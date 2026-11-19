"""The core loop: detect a real change, prove it, publish it once.

These tests use a fake fetch boundary, never the network. What they pin is the
honesty of the detector: a first poll is a baseline, an unchanged page is not
an alert, an empty page is an error, robots is obeyed, and nothing is public
until a human approves it.
"""

from app.core.clock import utcnow
from app.models.fact import FactModel
from app.models.notification import NotificationDeliveryModel, NotificationModel
from app.models.source import SourceModel
from app.models.subscription import SubscriptionModel
from app.services import poller
from app.services.poller import FetchResult

SOURCE_URL = "https://univ.example.ma/announcements"


def page(text: str, **kwargs) -> FetchResult:
    return FetchResult(
        200,
        f"<html><body><main><h1>إعلانات</h1><p>{text}</p></main></body></html>",
        **kwargs,
    )


class FakeFetcher:
    """A scripted fetch boundary. No socket is opened."""

    def __init__(self, pages, robots=None):
        self.pages = dict(pages)
        self.robots = robots
        self.calls = []

    def __call__(self, url, *, etag=None, last_modified=None):
        self.calls.append((url, etag, last_modified))
        if url.endswith("/robots.txt"):
            if self.robots is None:
                return FetchResult(404)
            return FetchResult(200, self.robots)
        item = self.pages[url]
        if callable(item):
            return item(etag=etag, last_modified=last_modified)
        return item


def _source(db, url: str = SOURCE_URL, **kwargs) -> SourceModel:
    source = SourceModel(
        id="src-test",
        name_ar="جامعة الاختبار — الإعلانات",
        url=url,
        check_interval_minutes=60,
        **kwargs,
    )
    db.add(source)
    db.commit()
    db.refresh(source)
    return source


def _poll(db, source, fetcher):
    return poller.poll_source(db, source, fetch=fetcher, now=utcnow())


def _changed_fact(db, *, is_official: bool = True) -> FactModel:
    """Baseline, then a real change, and return the pending fact."""
    source = _source(db, is_official=is_official)
    fetcher = FakeFetcher({SOURCE_URL: page("أجل الترشيح: 30 نوفمبر 2026")})
    assert _poll(db, source, fetcher).status == "baseline"

    fetcher.pages[SOURCE_URL] = page("أجل الترشيح: 15 ديسمبر 2026")
    outcome = _poll(db, source, fetcher)
    assert outcome.status == "changed"
    return db.query(FactModel).filter(FactModel.id == outcome.fact_id).one()


def test_first_poll_records_a_baseline_not_a_fact(db):
    source = _source(db)
    fetcher = FakeFetcher({SOURCE_URL: page("أجل الترشيح: 30 نوفمبر 2026")})

    outcome = _poll(db, source, fetcher)

    assert outcome.status == "baseline"
    assert outcome.fact_id is None
    assert db.query(FactModel).count() == 0
    assert source.last_hash is not None
    assert source.last_status == "baseline"


def test_second_poll_detects_the_change_and_creates_a_pending_fact(db):
    fact = _changed_fact(db)

    assert fact.status == "pending"
    assert fact.source_url == SOURCE_URL
    assert fact.verified_at is not None
    assert fact.snapshot_hash != fact.previous_hash
    assert "15 ديسمبر" in fact.diff_text
    assert "30 نوفمبر" in fact.diff_text
    assert "+" in fact.title_ar and "−" in fact.title_ar

    source = db.query(SourceModel).filter(SourceModel.id == fact.source_id).one()
    assert source.last_status == "changed"
    assert source.last_changed_at is not None


def test_unchanged_page_creates_no_fact(db):
    source = _source(db)
    fetcher = FakeFetcher({SOURCE_URL: page("نفس الإعلان")})

    assert _poll(db, source, fetcher).status == "baseline"
    assert _poll(db, source, fetcher).status == "not_modified"
    assert db.query(FactModel).count() == 0


def test_robots_disallow_blocks_the_poll(db):
    source = _source(db)
    fetcher = FakeFetcher(
        {SOURCE_URL: page("لن تُقرأ")},
        robots="User-agent: *\nDisallow: /announcements\n",
    )

    outcome = _poll(db, source, fetcher)

    assert outcome.status == "robots_denied"
    assert db.query(FactModel).count() == 0
    assert source.last_status == "robots_denied"
    # The page itself must not have been requested.
    assert all(url.endswith("/robots.txt") for url, _, _ in fetcher.calls)


def test_conditional_304_is_not_a_change(db):
    source = _source(db)
    state = {"calls": 0}

    def responder(etag=None, last_modified=None):
        state["calls"] += 1
        if state["calls"] == 1:
            return page("إعلان ثابت", etag='"v1"')
        return FetchResult(304)

    fetcher = FakeFetcher({SOURCE_URL: responder})
    assert _poll(db, source, fetcher).status == "baseline"
    assert _poll(db, source, fetcher).status == "not_modified"
    assert db.query(FactModel).count() == 0


def test_empty_page_is_an_error_not_a_false_change(db):
    source = _source(db)
    fetcher = FakeFetcher(
        {SOURCE_URL: FetchResult(200, "<html><body><script>x()</script></body></html>")}
    )

    outcome = _poll(db, source, fetcher)

    assert outcome.status == "error"
    assert db.query(FactModel).count() == 0
    assert source.last_status == "error"


def test_transport_error_is_recorded_and_not_published(db):
    source = _source(db)
    fetcher = FakeFetcher({SOURCE_URL: FetchResult(0, error="transport: ConnectError")})

    outcome = _poll(db, source, fetcher)

    assert outcome.status == "error"
    assert "transport" in (source.last_error or "")
    assert db.query(FactModel).count() == 0


def test_approve_publishes_and_queues_exactly_one_delivery(db):
    fact = _changed_fact(db)
    db.add(
        SubscriptionModel(
            id="sub-active",
            email="student@uit.ac.ma",
            status="active",
            confirmed_at=utcnow(),
        )
    )
    db.commit()

    notification = poller.approve_fact(
        db,
        fact,
        base_url="http://testserver",
        hmac_secret="test-hmac",
        dispatch=True,
    )

    assert fact.status == "approved"
    assert fact.notification_id == notification.id
    assert notification.kind == "official_change"
    assert notification.source_url == SOURCE_URL
    assert notification.verified_at == fact.verified_at
    assert notification.dedupe_key == f"fact:{fact.id}"

    deliveries = (
        db.query(NotificationDeliveryModel)
        .filter(NotificationDeliveryModel.notification_id == notification.id)
        .all()
    )
    assert len(deliveries) == 1
    assert deliveries[0].recipient_email == "student@uit.ac.ma"
    assert deliveries[0].status == "pending"
    assert f"/n/{fact.id}" in deliveries[0].body
    assert "صفحة المصدر" in deliveries[0].body
    assert "/api/v1/subscriptions/unsubscribe" in deliveries[0].body

    # Approving twice must not send twice.
    again = poller.approve_fact(
        db,
        fact,
        base_url="http://testserver",
        hmac_secret="test-hmac",
        dispatch=True,
    )
    assert again.id == notification.id
    assert (
        db.query(NotificationDeliveryModel)
        .filter(NotificationDeliveryModel.notification_id == notification.id)
        .count()
        == 1
    )


def test_non_official_source_is_not_labeled_official(db):
    fact = _changed_fact(db, is_official=False)

    notification = poller.approve_fact(
        db,
        fact,
        base_url="http://testserver",
        hmac_secret="test-hmac",
        dispatch=False,
    )

    assert notification.kind == "source_change"
    assert notification.kind != "official_change"


def test_reject_keeps_the_fact_private(db):
    fact = _changed_fact(db)

    poller.reject_fact(db, fact, note="تغيير قالب وليس إعلانًا")

    assert fact.status == "rejected"
    assert fact.review_note == "تغيير قالب وليس إعلانًا"
    assert db.query(NotificationModel).count() == 0
    assert db.query(NotificationDeliveryModel).count() == 0


def test_public_proof_page_renders_source_and_verified_at(client, db):
    fact = _changed_fact(db)
    poller.approve_fact(
        db,
        fact,
        base_url="http://testserver",
        hmac_secret="test-hmac",
        dispatch=False,
    )

    response = client.get(f"/n/{fact.id}")

    assert response.status_code == 200
    body = response.text
    assert SOURCE_URL in body
    assert fact.snapshot_hash in body
    assert fact.id in body
    assert fact.verified_at.strftime("%Y-%m-%d %H:%M") in body
    assert "واقعة موثّقة" in body
    assert "DKIM" in body  # the two proofs are explicitly separated
    assert "<script" not in body.lower()
    assert "default-src 'none'" in response.headers["content-security-policy"]


def test_pending_fact_is_not_public(client, db):
    fact = _changed_fact(db)

    response = client.get(f"/n/{fact.id}")

    assert response.status_code == 404
    assert "الواقعة غير متاحة" in response.text


def test_public_fact_listing_hides_pending_rows(client, db):
    fact = _changed_fact(db)

    assert client.get("/api/v1/facts").json() == []
    assert client.get("/api/v1/facts?status=pending").status_code == 401

    review = client.get(
        "/api/v1/facts?status=pending",
        headers={"X-Jami3ati-Admin": "test-admin-token"},
    )
    assert review.status_code == 200
    assert [row["id"] for row in review.json()] == [fact.id]


def test_source_registry_requires_admin_to_write(client):
    payload = {"name_ar": "إعلانات جامعة", "url": "https://univ.example.ma/news"}

    assert client.post("/api/v1/sources", json=payload).status_code == 401

    created = client.post(
        "/api/v1/sources",
        json=payload,
        headers={"X-Jami3ati-Admin": "test-admin-token"},
    )
    assert created.status_code == 201
    source_id = created.json()["id"]
    assert created.json()["last_hash"] is None

    # Reading the registry is public: a citizen may see what is watched.
    assert client.get("/api/v1/sources").status_code == 200

    # Polling is an admin action, and an unknown id is a 404 before any fetch.
    assert client.post(f"/api/v1/sources/{source_id}/poll").status_code == 401
    assert (
        client.post(
            "/api/v1/sources/src-missing/poll",
            headers={"X-Jami3ati-Admin": "test-admin-token"},
        ).status_code
        == 404
    )


def test_source_rejects_a_non_http_url(client):
    response = client.post(
        "/api/v1/sources",
        json={"name_ar": "مصدر", "url": "ftp://example.ma/file"},
        headers={"X-Jami3ati-Admin": "test-admin-token"},
    )
    assert response.status_code == 422
