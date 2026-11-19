"""The public proof page: `/n/{fact_id}`.

This is the product's central promise in one page: a stranger with no account
opens a stable URL and sees *what changed, on which official page, and when it
was read*. The page is plain HTML with no JavaScript and no external resource,
so it renders on a bad connection and cannot be used to track the reader.

It deliberately separates two claims that are often conflated:

* the **source proof** — `source_url` + `verified_at` + `snapshot_hash` — says
  the text matched an official page at a given moment;
* it says nothing about DKIM/SPF/DMARC, which prove only that an email came
  from our domain. The footer states this in plain Arabic.
"""

from datetime import datetime
from html import escape
from typing import Optional

from fastapi import APIRouter, Depends
from fastapi.responses import HTMLResponse
from sqlalchemy.orm import Session

from app.core.database import get_db
from app.models.fact import FactModel

router = APIRouter(tags=["Public proof"])

_CSP = (
    "default-src 'none'; style-src 'unsafe-inline'; img-src 'self' data:; "
    "base-uri 'none'; form-action 'none'; frame-ancestors 'none'"
)

_STYLE = """
:root { color-scheme: light; }
* { box-sizing: border-box; }
body {
  margin: 0; background: #f6f4ef; color: #1f2328;
  font-family: "IBM Plex Sans Arabic", "Segoe UI", Tahoma, sans-serif;
  line-height: 1.8;
}
main { max-width: 860px; margin: 0 auto; padding: 24px 16px 64px; }
header { padding: 8px 0 20px; }
.brand { font-size: 1.5rem; font-weight: 700; color: #0f5132; text-decoration: none; }
.sub { margin: 4px 0 0; color: #5c636a; font-size: .95rem; }
.card {
  background: #fff; border: 1px solid #e3e0d8; border-radius: 14px;
  padding: 20px; margin: 0 0 18px;
}
.badge {
  display: inline-block; margin: 0 0 10px; padding: 2px 10px;
  border-radius: 999px; background: #e6f4ea; color: #0f5132;
  font-size: .85rem; font-weight: 600;
}
h1 { font-size: 1.35rem; margin: 0 0 16px; }
h2 { font-size: 1.05rem; margin: 0 0 10px; }
dl { display: grid; grid-template-columns: max-content 1fr; gap: 8px 16px; margin: 0; }
dt { color: #5c636a; font-size: .9rem; }
dd { margin: 0; overflow-wrap: anywhere; }
a { color: #0b5ed7; }
code, pre { font-family: ui-monospace, Consolas, monospace; }
code { font-size: .82rem; }
pre {
  margin: 0; padding: 14px; background: #0f172a; color: #e6edf3;
  border-radius: 10px; overflow: auto; white-space: pre-wrap;
  font-size: .82rem; line-height: 1.6;
}
details summary { cursor: pointer; color: #0b5ed7; }
.note { background: #fff8e6; border-color: #f0d58c; }
.note p { margin: 0 0 10px; }
footer { color: #5c636a; font-size: .85rem; text-align: center; padding-top: 8px; }
@media (max-width: 560px) {
  dl { grid-template-columns: 1fr; gap: 2px; }
  dd { margin-bottom: 8px; }
}
"""


def _fmt(value: Optional[datetime]) -> str:
    if value is None:
        return "غير محدَّد"
    return value.strftime("%Y-%m-%d %H:%M") + " UTC"


def _shell(title: str, body: str, *, status_code: int = 200) -> HTMLResponse:
    document = f"""<!doctype html>
<html lang="ar" dir="rtl">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{escape(title)} — جامعتي</title>
<style>{_STYLE}</style>
</head>
<body>
<main>
<header>
<a class="brand" href="/">جامعتي</a>
<p class="sub">مرصد مواطن للبيانات المفتوحة في التعليم العالي</p>
</header>
{body}
<footer>
صفحة عامة بلا حساب. الواقعة المسجّلة لا تتغيّر حتى لو تغيّر المصدر لاحقًا.
</footer>
</main>
</body>
</html>"""
    return HTMLResponse(
        content=document,
        status_code=status_code,
        headers={
            "Content-Security-Policy": _CSP,
            "X-Content-Type-Options": "nosniff",
            "Referrer-Policy": "no-referrer",
            "Cache-Control": "public, max-age=60",
        },
    )


def _not_found() -> HTMLResponse:
    body = """
<section class="card">
<h1>الواقعة غير متاحة</h1>
<p>لا توجد واقعة منشورة بهذا المعرّف. قد يكون الرابط غير صحيح، أو أن الواقعة
ما زالت قيد المراجعة ولم تُنشر بعد.</p>
</section>
"""
    return _shell("الواقعة غير متاحة", body, status_code=404)


@router.get("/n/{fact_id}", response_class=HTMLResponse)
def proof_page(fact_id: str, db: Session = Depends(get_db)):
    fact = db.query(FactModel).filter(FactModel.id == fact_id).first()
    # Pending and rejected facts are not public: the proof page must never be
    # the place a false positive leaks out.
    if fact is None or fact.status != "approved":
        return _not_found()

    previous = (
        f"<dt>البصمة السابقة</dt><dd dir=\"ltr\"><code>{escape(fact.previous_hash)}</code></dd>"
        if fact.previous_hash
        else ""
    )

    body = f"""
<section class="card">
<p class="badge">واقعة موثّقة</p>
<h1>{escape(fact.title_ar)}</h1>
<dl>
<dt>المصدر</dt><dd>{escape(fact.source_name)}</dd>
<dt>رابط الصفحة المصدر</dt>
<dd dir="ltr"><a href="{escape(fact.source_url)}" rel="noopener noreferrer nofollow" target="_blank">{escape(fact.source_url)}</a></dd>
<dt>تاريخ قراءة المصدر</dt><dd dir="ltr">{escape(_fmt(fact.verified_at))}</dd>
<dt>معرّف الواقعة</dt><dd dir="ltr"><code>{escape(fact.id)}</code></dd>
<dt>بصمة اللقطة (SHA-256)</dt><dd dir="ltr"><code>{escape(fact.snapshot_hash)}</code></dd>
{previous}
</dl>
</section>

<section class="card">
<h2>الفرق المرصود</h2>
<pre dir="auto">{escape(fact.diff_text)}</pre>
</section>

<section class="card">
<details>
<summary>عرض اللقطة الكاملة التي وُلِّدت منها البصمة</summary>
<pre dir="auto">{escape(fact.snapshot_text)}</pre>
</details>
</section>

<section class="card note">
<h2>ما الذي تثبته هذه الصفحة؟</h2>
<p>تثبت أن النص أعلاه طابق الصفحة المصدر في التاريخ المذكور، وأن بصمة اللقطة
هي البصمة المعروضة. يمكنك فتح رابط المصدر ومقارنة ما تراه اليوم بما هو
مسجَّل هنا.</p>
<p>هذه الصفحة لا تثبت أن الجهة المصدرة وقّعت هذه الرسالة. التحقق من المصدر
(<span dir="ltr">source_url + verified_at</span>) شيء، وتوقيع البريد
(<span dir="ltr">DKIM/SPF/DMARC</span>) شيء آخر: التوقيع يثبت أن الرسالة
خرجت من نطاقنا، لا أن المعلومة صادرة عن الوزارة.</p>
</section>
"""
    return _shell(fact.title_ar, body)
