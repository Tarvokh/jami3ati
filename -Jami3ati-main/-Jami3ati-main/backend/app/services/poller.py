"""The change detector: the part of Jami3ati that produces facts.

The poller is deliberately small and honest. It watches one official page at a
time, keeps a normalized snapshot, and only records a fact when the *text* of
the page actually changed. It is not a crawler and it does not guess:

* the first successful poll records a **baseline**, never a "change";
* a page that cannot be read (transport error, HTTP error, empty/JS-only body)
  is recorded as an error, never as a change;
* `robots.txt` is fetched and obeyed for every poll;
* `ETag` / `Last-Modified` are sent so a polite server can answer `304`;
* a detected change is written as `pending` and is **not** public until a human
  approves it (`approve_fact`). That human-in-the-loop step is the honest
  answer to false positives, and the pitch says so instead of claiming full
  automation.

The normalized-text comparison is not magic. A redesigned template can still
look like a change; the diff is shown to the reviewer precisely so that noise
is rejected before it becomes a public fact.
"""

import codecs
import difflib
import hashlib
import logging
import re
import uuid
from dataclasses import dataclass
from datetime import datetime, timedelta
from html.parser import HTMLParser
from typing import Callable, List, Optional
from urllib.parse import urlsplit, urlunsplit
from urllib.robotparser import RobotFileParser

import httpx
from sqlalchemy.orm import Session

from app.core.clock import utcnow
from app.models.fact import FactModel
from app.models.notification import NotificationModel
from app.models.source import SourceModel
from app.services import deliveries

logger = logging.getLogger("jami3ati.poller")

DEFAULT_USER_AGENT = "Jami3atiBot/0.1 (+https://jami3ati.ma/bot)"
DEFAULT_TIMEOUT_SECONDS = 15.0

MAX_SNAPSHOT_CHARS = 60_000
MAX_DIFF_CHARS = 12_000
MAX_DIFF_LINES = 200


# --------------------------------------------------------------------------
# Fetch boundary
# --------------------------------------------------------------------------


@dataclass(frozen=True)
class FetchResult:
    """What a fetch returned, in the smallest shape the poller needs.

    `status_code == 0` means the request never produced an HTTP answer; `error`
    then carries a short reason. Tests replace this boundary with a fake, which
    is why the poller never calls `httpx` directly.
    """

    status_code: int
    text: str = ""
    etag: Optional[str] = None
    last_modified: Optional[str] = None
    error: str = ""


Fetcher = Callable[..., FetchResult]


def http_fetch(
    url: str,
    *,
    etag: Optional[str] = None,
    last_modified: Optional[str] = None,
    user_agent: str = DEFAULT_USER_AGENT,
    timeout_seconds: float = DEFAULT_TIMEOUT_SECONDS,
) -> FetchResult:
    headers = {
        "User-Agent": user_agent,
        "Accept": "text/html,application/xhtml+xml;q=0.9,*/*;q=0.1",
    }
    if etag:
        headers["If-None-Match"] = etag
    if last_modified:
        headers["If-Modified-Since"] = last_modified

    try:
        response = httpx.get(
            url,
            headers=headers,
            timeout=timeout_seconds,
            follow_redirects=True,
        )
    except httpx.HTTPError as exc:
        return FetchResult(0, error=f"transport: {type(exc).__name__}")

    return FetchResult(
        status_code=response.status_code,
        text=_decode_body(response),
        etag=response.headers.get("etag"),
        last_modified=response.headers.get("last-modified"),
    )


def _decode_body(response: httpx.Response) -> str:
    """Decode without turning a missing or wrong charset into mojibake.

    Many Moroccan sites omit `charset`, or declare a legacy one
    (`iso-8859-1`, `windows-1252`) while actually serving UTF-8 bytes.
    A byte-order mark is the one signal that is never wrong, so it is
    honoured first; otherwise UTF-8 wins over a declared legacy charset.
    A genuine single-byte page still decodes through its declared
    charset or the Windows-1252 fallback.
    """
    content = response.content
    declared = (response.encoding or "").lower()

    for bom, encoding in (
        (codecs.BOM_UTF32_LE, "utf-32"),
        (codecs.BOM_UTF32_BE, "utf-32"),
        (codecs.BOM_UTF8, "utf-8-sig"),
        (codecs.BOM_UTF16_LE, "utf-16"),
        (codecs.BOM_UTF16_BE, "utf-16"),
    ):
        if content.startswith(bom):
            try:
                return content.decode(encoding)
            except (LookupError, UnicodeDecodeError):
                break

    try:
        return content.decode("utf-8")
    except UnicodeDecodeError:
        pass

    if declared:
        try:
            return content.decode(declared)
        except (LookupError, UnicodeDecodeError):
            pass

    try:
        return content.decode("windows-1252")
    except UnicodeDecodeError:
        return content.decode("utf-8", errors="replace")


# --------------------------------------------------------------------------
# Normalization and hashing
# --------------------------------------------------------------------------


class _TextExtractor(HTMLParser):
    """Collect visible text, skipping code and non-content elements."""

    _SKIP = {"script", "style", "noscript", "svg", "template"}
    _BREAK = {
        "p",
        "div",
        "br",
        "li",
        "tr",
        "td",
        "th",
        "h1",
        "h2",
        "h3",
        "h4",
        "h5",
        "h6",
        "section",
        "article",
        "header",
        "footer",
        "nav",
        "main",
        "table",
        "ul",
        "ol",
        "dl",
        "blockquote",
    }

    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self._skip_depth = 0
        self.parts: List[str] = []

    def handle_starttag(self, tag: str, attrs) -> None:
        if tag in self._SKIP:
            self._skip_depth += 1
            return
        if tag in self._BREAK:
            self.parts.append("\n")

    def handle_endtag(self, tag: str) -> None:
        if tag in self._SKIP:
            if self._skip_depth:
                self._skip_depth -= 1
            return
        if tag in self._BREAK:
            self.parts.append("\n")

    def handle_data(self, data: str) -> None:
        if not self._skip_depth:
            self.parts.append(data)


_WHITESPACE = re.compile(r"[ \t\u00a0\u200e\u200f]+")


def normalize_text(raw_html: str) -> str:
    """Reduce an HTML page to stable, comparable text.

    Lines are trimmed, internal horizontal whitespace is collapsed, and empty
    lines are dropped. The result is deterministic for the same markup, which
    is what makes the hash meaningful.
    """
    extractor = _TextExtractor()
    extractor.feed(raw_html)
    extractor.close()

    lines: List[str] = []
    for line in "".join(extractor.parts).splitlines():
        cleaned = _WHITESPACE.sub(" ", line).strip()
        if cleaned:
            lines.append(cleaned)
    return "\n".join(lines)[:MAX_SNAPSHOT_CHARS]


def content_hash(normalized_text: str) -> str:
    return hashlib.sha256(normalized_text.encode("utf-8")).hexdigest()


def build_diff(previous: str, current: str) -> tuple[int, int, str]:
    """Return (added, removed, truncated unified diff)."""
    diff_lines = list(
        difflib.unified_diff(
            previous.splitlines(),
            current.splitlines(),
            fromfile="السابق",
            tofile="الحالي",
            lineterm="",
            n=2,
        )
    )
    added = sum(
        1 for line in diff_lines if line.startswith("+") and not line.startswith("+++")
    )
    removed = sum(
        1 for line in diff_lines if line.startswith("-") and not line.startswith("---")
    )
    text = "\n".join(diff_lines[:MAX_DIFF_LINES])
    if len(text) > MAX_DIFF_CHARS:
        text = text[:MAX_DIFF_CHARS] + "\n… (الفرق مقتطع)"
    return added, removed, text


# --------------------------------------------------------------------------
# robots.txt
# --------------------------------------------------------------------------


def robots_allowed(fetch: Fetcher, url: str, user_agent: str) -> bool:
    """Honor robots.txt. A missing or unreachable file is treated as allow.

    That fail-open choice is deliberate: most official pages have no
    robots.txt, and refusing to poll them would make the product useless. An
    explicit `Disallow` is always obeyed.
    """
    parts = urlsplit(url)
    if parts.scheme not in ("http", "https") or not parts.netloc:
        return False

    robots_url = urlunsplit((parts.scheme, parts.netloc, "/robots.txt", "", ""))
    result = fetch(robots_url)
    if result.status_code == 0 or result.status_code >= 400:
        return True
    if not result.text.strip():
        return True

    parser = RobotFileParser()
    parser.parse(result.text.splitlines())
    return parser.can_fetch(user_agent, url)


# --------------------------------------------------------------------------
# Polling
# --------------------------------------------------------------------------


@dataclass(frozen=True)
class PollOutcome:
    source_id: str
    # baseline | not_modified | changed | robots_denied | inactive | error
    status: str
    fact_id: Optional[str] = None
    detail: str = ""


def _new_fact_id() -> str:
    return f"fact-{uuid.uuid4().hex[:12]}"


def _fmt_utc(value: datetime) -> str:
    return value.strftime("%Y-%m-%d %H:%M") + " UTC"


def poll_source(
    db: Session,
    source: SourceModel,
    *,
    fetch: Fetcher = http_fetch,
    user_agent: str = DEFAULT_USER_AGENT,
    now: Optional[datetime] = None,
    commit: bool = True,
) -> PollOutcome:
    """Read one source once and record what happened.

    No exception escapes for a bad page: the source row carries the error so
    the admin panel can show a silent poller instead of pretending it works.
    """
    moment = now or utcnow()
    source.last_checked_at = moment

    if not source.is_active:
        source.last_status = "inactive"
        if commit:
            db.commit()
        return PollOutcome(source.id, "inactive")

    if not robots_allowed(fetch, source.url, user_agent):
        source.last_status = "robots_denied"
        source.last_error = "robots.txt disallows this path"
        if commit:
            db.commit()
        logger.warning("robots.txt denied polling %s", source.url)
        return PollOutcome(source.id, "robots_denied", detail=source.last_error)

    result = fetch(
        source.url,
        etag=source.etag,
        last_modified=source.last_modified,
    )

    if result.status_code == 0:
        source.last_status = "error"
        source.last_error = result.error or "transport error"
        if commit:
            db.commit()
        return PollOutcome(source.id, "error", detail=source.last_error)

    if result.status_code == 304:
        source.last_status = "not_modified"
        source.last_error = None
        if commit:
            db.commit()
        return PollOutcome(source.id, "not_modified")

    if result.status_code >= 400:
        source.last_status = "error"
        source.last_error = f"http {result.status_code}"
        if commit:
            db.commit()
        return PollOutcome(source.id, "error", detail=source.last_error)

    normalized = normalize_text(result.text)
    if not normalized:
        # A JavaScript-only or blocked page yields no text. Recording that as a
        # change would be a false alert; recording it as "no change" would be a
        # silent miss. It is an error, and the admin sees it.
        source.last_status = "error"
        source.last_error = "empty page after normalization"
        if commit:
            db.commit()
        return PollOutcome(source.id, "error", detail=source.last_error)

    new_hash = content_hash(normalized)

    if source.last_hash is None:
        # First observation: this is the baseline, not a change.
        source.last_hash = new_hash
        source.last_text = normalized
        source.etag = result.etag
        source.last_modified = result.last_modified
        source.last_status = "baseline"
        source.last_error = None
        if commit:
            db.commit()
        return PollOutcome(source.id, "baseline")

    if new_hash == source.last_hash:
        source.etag = result.etag
        source.last_modified = result.last_modified
        source.last_status = "not_modified"
        source.last_error = None
        if commit:
            db.commit()
        return PollOutcome(source.id, "not_modified")

    previous_text = source.last_text or ""
    added, removed, diff_text = build_diff(previous_text, normalized)
    verified_at = moment

    fact = FactModel(
        id=_new_fact_id(),
        source_id=source.id,
        institution_id=source.institution_id,
        source_url=source.url,
        source_name=source.name_ar,
        title_ar=f"تغيّر على {source.name_ar}: +{added}/−{removed} سطر",
        body_ar=(
            "اكتشف مرصد جامعتي تغييرًا على صفحة مصدر مسجَّل.\n\n"
            f"المصدر: {source.name_ar}\n"
            f"الرابط: {source.url}\n"
            f"تاريخ القراءة: {_fmt_utc(verified_at)}\n"
            f"بصمة اللقطة: {new_hash}\n"
            f"البصمة السابقة: {source.last_hash}\n\n"
            f"ملخص الفرق: +{added} / −{removed} سطر\n\n"
            f"{diff_text}\n"
        ),
        diff_text=diff_text,
        snapshot_hash=new_hash,
        previous_hash=source.last_hash,
        snapshot_text=normalized,
        status="pending",
        verified_at=verified_at,
    )
    db.add(fact)

    source.last_hash = new_hash
    source.last_text = normalized
    source.etag = result.etag
    source.last_modified = result.last_modified
    source.last_status = "changed"
    source.last_error = None
    source.last_changed_at = verified_at

    if commit:
        db.commit()
    logger.info("source %s changed; pending fact %s", source.id, fact.id)
    return PollOutcome(source.id, "changed", fact_id=fact.id)


def poll_due_sources(
    db: Session,
    *,
    fetch: Fetcher = http_fetch,
    user_agent: str = DEFAULT_USER_AGENT,
    now: Optional[datetime] = None,
    limit: int = 10,
) -> List[PollOutcome]:
    """Poll active sources whose interval has elapsed. Never raises per source."""
    moment = now or utcnow()
    candidates = (
        db.query(SourceModel)
        .filter(SourceModel.is_active.is_(True))
        .order_by(SourceModel.last_checked_at.asc().nullsfirst())
        .all()
    )

    outcomes: List[PollOutcome] = []
    for source in candidates:
        if len(outcomes) >= limit:
            break
        if source.last_checked_at is not None:
            due_at = source.last_checked_at + timedelta(
                minutes=max(source.check_interval_minutes or 0, 1)
            )
            if due_at > moment:
                continue
        try:
            outcomes.append(poll_source(db, source, fetch=fetch, user_agent=user_agent, now=moment))
        except Exception:  # noqa: BLE001 - one bad source must not stop the loop
            db.rollback()
            logger.exception("poll failed for source %s", source.id)
    return outcomes


# --------------------------------------------------------------------------
# Human review
# --------------------------------------------------------------------------


def approve_fact(
    db: Session,
    fact: FactModel,
    *,
    base_url: str,
    hmac_secret: str,
    dispatch: bool = True,
) -> NotificationModel:
    """Turn an approved fact into the public notification and its deliveries.

    Idempotent: approving twice returns the same notification instead of
    fanning out a second email.
    """
    if fact.status == "rejected":
        raise ValueError("a rejected fact cannot be approved")

    if fact.status == "approved" and fact.notification_id:
        existing = (
            db.query(NotificationModel)
            .filter(NotificationModel.id == fact.notification_id)
            .first()
        )
        if existing is not None:
            return existing

    source = db.query(SourceModel).filter(SourceModel.id == fact.source_id).first()
    is_official = True if source is None else bool(source.is_official)

    notification = NotificationModel(
        id=f"ntf-{uuid.uuid4().hex[:12]}",
        institution_id=fact.institution_id,
        kind="official_change" if is_official else "source_change",
        title_ar=fact.title_ar,
        body_ar=(
            f"{fact.body_ar}\n"
            f"صفحة الإثبات العامة: {base_url}/n/{fact.id}\n\n"
            "هذا تنبيه عن تغيّر رُصد على صفحة مصدر، وليس توقيعًا من الجهة المصدرة.\n"
        ),
        source_url=fact.source_url,
        verified_at=fact.verified_at,
        dedupe_key=f"fact:{fact.id}",
    )
    db.add(notification)

    if dispatch:
        deliveries.fan_out_notification(
            db,
            notification,
            base_url=base_url,
            hmac_secret=hmac_secret,
        )

    fact.status = "approved"
    fact.notification_id = notification.id
    fact.approved_at = utcnow()
    db.commit()
    db.refresh(notification)
    return notification


def reject_fact(db: Session, fact: FactModel, *, note: str = "") -> FactModel:
    if fact.status == "approved":
        raise ValueError("an approved fact cannot be rejected")
    fact.status = "rejected"
    fact.review_note = note.strip() or None
    db.commit()
    db.refresh(fact)
    return fact
