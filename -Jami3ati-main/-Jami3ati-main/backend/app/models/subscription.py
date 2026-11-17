from sqlalchemy import Column, String, DateTime
from app.core.clock import utcnow
from app.core.database import Base


class SubscriptionModel(Base):
    """One row per email address, not per (email, institution) pair.

    `email` is unique on purpose: a second signup for the same address must
    rotate the pending token (resend) or reactivate an unsubscribed row, not
    create a duplicate that could later receive the same alert twice.
    """

    __tablename__ = "subscriptions"

    id = Column(String, primary_key=True, index=True)
    email = Column(String, unique=True, index=True, nullable=False)
    institution_id = Column(String, index=True, nullable=True)
    status = Column(String, default="pending", index=True, nullable=False)
    confirm_token_hash = Column(String, nullable=True)
    confirm_expires_at = Column(DateTime, nullable=True)
    confirmed_at = Column(DateTime, nullable=True)
    unsubscribed_at = Column(DateTime, nullable=True)
    created_at = Column(DateTime, default=utcnow, nullable=False)
    updated_at = Column(DateTime, default=utcnow, onupdate=utcnow, nullable=False)
