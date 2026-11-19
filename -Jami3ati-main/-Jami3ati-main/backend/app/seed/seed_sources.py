"""Register the real pages the observatory watches.

Nothing here is invented: every URL is a public page that was fetched during
setup. Running with `--poll` also records the first snapshot as a baseline, so
the next real change becomes the first fact.

Usage (against the demo database):

    DATABASE_URL=postgresql://postgres@127.0.0.1:55432/jami3ati_demo \
        python app/seed/seed_sources.py --poll
"""

import sys

from app.core.database import Base, SessionLocal, engine
from app.models import fact, institution, notification, report, source, subscription, university  # noqa: F401
from app.models.source import SourceModel
from app.services import poller

# One real source, deliberately narrow: the admissions category of Université
# Ibn Tofail. It is public, it is served over https, and it changes when the
# university publishes an admission announcement.
DEMO_SOURCES = [
    {
        "id": "src-uit-admission",
        "name_ar": "جامعة ابن طفيل — إعلانات القبول والتسجيل",
        "url": "https://www.uit.ac.ma/category/admission/",
        "institution_id": None,
        "check_interval_minutes": 360,
        "is_active": True,
    },
]


def seed_sources(*, poll: bool = False) -> None:
    Base.metadata.create_all(bind=engine)
    db = SessionLocal()
    try:
        for item in DEMO_SOURCES:
            existing = (
                db.query(SourceModel).filter(SourceModel.url == item["url"]).first()
            )
            if existing is None:
                db.add(SourceModel(**item))
                db.commit()
                print(f"registered {item['id']} -> {item['url']}")
            else:
                print(f"already registered {existing.id} -> {existing.url}")

        if poll:
            for item in DEMO_SOURCES:
                row = (
                    db.query(SourceModel)
                    .filter(SourceModel.url == item["url"])
                    .one()
                )
                outcome = poller.poll_source(db, row)
                print(
                    f"poll {row.id}: status={outcome.status} "
                    f"fact={outcome.fact_id or '-'} hash={(row.last_hash or '-')[:16]}"
                )
    finally:
        db.close()


if __name__ == "__main__":
    seed_sources(poll="--poll" in sys.argv)
