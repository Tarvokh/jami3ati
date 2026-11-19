from sqlalchemy import Column, DateTime, ForeignKey, String, Text
from app.core.clock import utcnow
from app.core.database import Base


class FactModel(Base):
    """One immutable observation of a source change.

    The table is called `fact_history` because rows are append-only: a fact is
    never edited after the source was read. `snapshot_text` is the normalized
    text that produced `snapshot_hash`, which is what lets a stranger check
    that the diff shown is the diff that was observed.

    `status` is the human-in-the-loop gate: `pending` facts are not public and
    are never sent, `approved` facts become a notification and are published at
    `/n/{id}`, `rejected` facts stay recorded but are not published.
    """

    __tablename__ = "fact_history"

    id = Column(String, primary_key=True, index=True)
    source_id = Column(String, ForeignKey("sources.id"), index=True, nullable=False)
    institution_id = Column(String, index=True, nullable=True)

    source_url = Column(String, nullable=False)
    source_name = Column(String, nullable=False)
    title_ar = Column(String, nullable=False)
    body_ar = Column(Text, nullable=False)
    diff_text = Column(Text, nullable=False)

    snapshot_hash = Column(String, nullable=False, index=True)
    previous_hash = Column(String, nullable=True)
    snapshot_text = Column(Text, nullable=False)

    status = Column(String, default="pending", index=True, nullable=False)
    notification_id = Column(String, index=True, nullable=True)
    review_note = Column(Text, nullable=True)

    # The moment the source was actually read. This is the claim the proof page
    # makes; it is not a DKIM/SPF/DMARC signature and must never be described
    # as one.
    verified_at = Column(DateTime, nullable=False)
    created_at = Column(DateTime, default=utcnow, nullable=False)
    approved_at = Column(DateTime, nullable=True)
