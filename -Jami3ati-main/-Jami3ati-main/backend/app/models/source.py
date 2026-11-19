from sqlalchemy import Boolean, Column, DateTime, Integer, String, Text
from app.core.clock import utcnow
from app.core.database import Base


class SourceModel(Base):
    """One official page the observatory watches.

    A source is a *page*, not an institution: the same institution may publish
    deadlines on several pages, and each page changes on its own schedule.
    `last_hash`/`last_text` are the baseline the next poll compares against, so
    the first successful poll records a baseline instead of inventing a change.
    """

    __tablename__ = "sources"

    id = Column(String, primary_key=True, index=True)
    name_ar = Column(String, nullable=False)
    url = Column(String, nullable=False, unique=True)
    institution_id = Column(String, index=True, nullable=True)
    is_active = Column(Boolean, default=True, nullable=False)
    # Whether the page is an official institutional source. The registry is
    # meant for official pages, but a rehearsal or aggregator source must not be
    # published with the «official change» label. Defaults to true so existing
    # rows keep their meaning.
    is_official = Column(Boolean, default=True, nullable=False)
    check_interval_minutes = Column(Integer, default=360, nullable=False)

    last_checked_at = Column(DateTime, nullable=True)
    last_changed_at = Column(DateTime, nullable=True)
    # ok | baseline | not_modified | changed | robots_denied | error
    last_status = Column(String, nullable=True)
    last_error = Column(Text, nullable=True)

    etag = Column(String, nullable=True)
    last_modified = Column(String, nullable=True)
    last_hash = Column(String, nullable=True)
    last_text = Column(Text, nullable=True)

    created_at = Column(DateTime, default=utcnow, nullable=False)
