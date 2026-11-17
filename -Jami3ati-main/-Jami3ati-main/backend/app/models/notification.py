from sqlalchemy import Column, String, DateTime, Integer, Text
from app.core.clock import utcnow
from app.core.database import Base


class NotificationModel(Base):
    """A fact from an official source — never a guess.

    `source_url` is mandatory and `verified_at` is the date the source was
    actually read. This is deliberately *not* DKIM/SPF/DMARC: those prove the
    email came from our domain, not that the fact came from the ministry.
    """

    __tablename__ = "notifications"

    id = Column(String, primary_key=True, index=True)
    institution_id = Column(String, index=True, nullable=True)
    kind = Column(String, index=True, nullable=False)
    title_ar = Column(String, nullable=False)
    body_ar = Column(Text, nullable=False)
    source_url = Column(String, nullable=False)
    verified_at = Column(DateTime, nullable=True)
    source_notification_id = Column(String, nullable=True)
    dedupe_key = Column(String, unique=True, index=True, nullable=True)
    created_at = Column(DateTime, default=utcnow, nullable=False)


class NotificationDeliveryModel(Base):
    """The outbox row. One per (notification, recipient).

    The rendered `subject`/`body` are frozen at enqueue time: editing a
    notification afterwards must not change what was already promised to a
    subscriber. `idempotency_key` is unique so a retried enqueue cannot create
    a second send for the same intent.
    """

    __tablename__ = "notification_deliveries"

    id = Column(String, primary_key=True, index=True)
    notification_id = Column(String, index=True, nullable=True)
    subscription_id = Column(String, index=True, nullable=True)
    purpose = Column(String, index=True, nullable=False)
    recipient_email = Column(String, index=True, nullable=False)
    subject = Column(String, nullable=False)
    body = Column(Text, nullable=False)
    idempotency_key = Column(String, unique=True, index=True, nullable=False)
    correlation_id = Column(String, nullable=False)
    list_unsubscribe = Column(String, nullable=True)
    reply_to = Column(String, nullable=True)
    status = Column(String, default="pending", index=True, nullable=False)
    attempts = Column(Integer, default=0, nullable=False)
    next_attempt_at = Column(DateTime, default=utcnow, index=True, nullable=False)
    tira_message_id = Column(String, nullable=True)
    last_error = Column(Text, nullable=True)
    accepted_at = Column(DateTime, nullable=True)
    created_at = Column(DateTime, default=utcnow, nullable=False)
    updated_at = Column(DateTime, default=utcnow, onupdate=utcnow, nullable=False)
