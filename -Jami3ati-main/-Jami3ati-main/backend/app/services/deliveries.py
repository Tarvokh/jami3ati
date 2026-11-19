"""Outbox rendering, fan-out, and the delivery worker.

Everything here is deliberately explicit about the two different claims:

* `accepted` means Tira holds a durable queue row for this message.
* `accepted` does **not** mean the student received it.

The worker is the only writer of `accepted`/`failed`/`dead`, so the status
column always answers "what did the delivery layer say", never "what did we
hope".
"""

import logging
import uuid
from datetime import datetime, timedelta
from typing import Iterable, Optional

from sqlalchemy.orm import Session

from app.core import tokens
from app.core.clock import utcnow
from app.models.notification import NotificationDeliveryModel, NotificationModel
from app.models.subscription import SubscriptionModel
from app.services import tira_outbound

logger = logging.getLogger("jami3ati.deliveries")

PURPOSE_CONFIRM = "confirm"
PURPOSE_NOTIFICATION = "notification"
PURPOSE_MODERATOR = "moderator"

STATUS_PENDING = "pending"
STATUS_ACCEPTED = "accepted"
STATUS_FAILED = "failed"
STATUS_DEAD = "dead"


def _new_id(prefix: str) -> str:
    return f"{prefix}-{uuid.uuid4().hex[:12]}"


def _backoff(attempts: int) -> timedelta:
    """30s, 60s, 120s, ... capped at 15 minutes."""
    seconds = min(30 * (2 ** max(attempts - 1, 0)), 900)
    return timedelta(seconds=seconds)


def _fmt_utc(value: Optional[datetime]) -> str:
    if value is None:
        return "غير محدَّد"
    return value.strftime("%Y-%m-%d %H:%M") + " UTC"


# --------------------------------------------------------------------------
# Message rendering
# --------------------------------------------------------------------------


def render_confirmation(
    subscription: SubscriptionModel,
    *,
    base_url: str,
    raw_token: str,
) -> tuple[str, str]:
    link = tokens.confirmation_link(base_url, subscription.id, raw_token)
    subject = "تأكيد الاشتراك في تنبيهات مرصد جامعتي"
    body = (
        "مرحبًا،\n\n"
        "طلبت الاشتراك في تنبيهات «مرصد جامعتي» — تنبيهات مبنية على صفحات مصادر "
        "مسجَّلة قابلة للتحقق.\n\n"
        "لتأكيد الاشتراك، افتح هذا الرابط:\n"
        f"{link}\n\n"
        f"الرابط صالح حتى {_fmt_utc(subscription.confirm_expires_at)}.\n"
        "إن لم تطلب الاشتراك، تجاهل هذه الرسالة ولن يُضاف بريدك إلى أي قائمة.\n\n"
        "— مرصد جامعتي\n"
    )
    return subject, body


def render_notification(
    notification: NotificationModel,
    *,
    base_url: str,
    hmac_secret: str,
    subscription_id: str,
) -> tuple[str, str]:
    unsubscribe = tokens.unsubscribe_link(base_url, hmac_secret, subscription_id)
    verified = _fmt_utc(notification.verified_at)
    subject = notification.title_ar
    body = (
        f"{notification.title_ar}\n\n"
        f"{notification.body_ar}\n\n"
        f"صفحة المصدر: {notification.source_url}\n"
        f"تاريخ التحقق من المصدر: {verified}\n\n"
        "أنت تتلقى هذه الرسالة لأنك أكّدت اشتراكك في تنبيهات مرصد جامعتي.\n"
        f"لإلغاء الاشتراك: {unsubscribe}\n\n"
        "— مرصد جامعتي\n"
    )
    return subject, body


def render_moderator_report(
    *,
    report_id: str,
    target_name: str,
    field_name: str,
    source_url: str,
    correction: str,
) -> tuple[str, str]:
    subject = f"بلاغ جديد ينتظر المراجعة: {target_name}"
    body = (
        f"وصل بلاغ جديد إلى مرصد جامعتي.\n\n"
        f"المعرّف: {report_id}\n"
        f"المؤسسة: {target_name}\n"
        f"الحقل: {field_name}\n"
        f"التصحيح المقترح: {correction}\n"
        f"رابط الإثبات: {source_url}\n\n"
        "البلاغ لا يُنشر ولا يُعتمد قبل مراجعة بشرية.\n\n"
        "— مرصد جامعتي\n"
    )
    return subject, body


# --------------------------------------------------------------------------
# Enqueue
# --------------------------------------------------------------------------


def _enqueue(
    db: Session,
    *,
    purpose: str,
    recipient_email: str,
    subject: str,
    body: str,
    idempotency_key: str,
    correlation_id: str,
    notification_id: Optional[str] = None,
    subscription_id: Optional[str] = None,
    list_unsubscribe: Optional[str] = None,
    reply_to: Optional[str] = None,
) -> NotificationDeliveryModel:
    """Insert one outbox row, idempotently.

    If the key already exists the existing row is returned untouched: the
    caller may retry the whole operation without producing a second email.
    """
    existing = (
        db.query(NotificationDeliveryModel)
        .filter(NotificationDeliveryModel.idempotency_key == idempotency_key)
        .first()
    )
    if existing is not None:
        return existing

    row = NotificationDeliveryModel(
        id=_new_id("dlv"),
        notification_id=notification_id,
        subscription_id=subscription_id,
        purpose=purpose,
        recipient_email=recipient_email,
        subject=subject,
        body=body,
        idempotency_key=idempotency_key,
        correlation_id=correlation_id,
        list_unsubscribe=list_unsubscribe,
        reply_to=reply_to,
        status=STATUS_PENDING,
        attempts=0,
        next_attempt_at=utcnow(),
    )
    db.add(row)
    return row


def enqueue_confirmation(
    db: Session,
    subscription: SubscriptionModel,
    *,
    base_url: str,
    raw_token: str,
) -> NotificationDeliveryModel:
    subject, body = render_confirmation(
        subscription, base_url=base_url, raw_token=raw_token
    )
    # The confirmation email is transactional, not a list message, so it
    # carries no List-Unsubscribe header. It carries an explicit "ignore this"
    # instruction instead.
    key = f"jami3ati:confirm:{subscription.id}:{tokens.confirm_token_digest(raw_token)[:12]}"
    return _enqueue(
        db,
        purpose=PURPOSE_CONFIRM,
        recipient_email=subscription.email,
        subject=subject,
        body=body,
        idempotency_key=key,
        correlation_id=f"confirm:{subscription.id}",
        subscription_id=subscription.id,
    )


def fan_out_notification(
    db: Session,
    notification: NotificationModel,
    *,
    base_url: str,
    hmac_secret: str,
) -> list[NotificationDeliveryModel]:
    """One delivery per *active* subscription.

    `pending` rows are skipped: an unconfirmed address must never receive a
    real alert. `unsubscribed` rows are skipped for the obvious reason.
    """
    query = db.query(SubscriptionModel).filter(SubscriptionModel.status == "active")
    if notification.institution_id:
        # A scoped subscriber still gets institution-wide news only when the
        # notification names that institution.
        query = query.filter(
            (SubscriptionModel.institution_id == notification.institution_id)
            | (SubscriptionModel.institution_id.is_(None))
        )

    created: list[NotificationDeliveryModel] = []
    for subscription in query.all():
        subject, body = render_notification(
            notification,
            base_url=base_url,
            hmac_secret=hmac_secret,
            subscription_id=subscription.id,
        )
        # `list_unsubscribe` must be <https://...> or <mailto:...> per Tira's
        # closed set. A local http base URL cannot satisfy that, so the header
        # is omitted there and the body link remains the working path.
        header_unsubscribe: Optional[str] = None
        if base_url.startswith("https://"):
            header_unsubscribe = (
                "<"
                + tokens.unsubscribe_link(base_url, hmac_secret, subscription.id)
                + ">"
            )
        created.append(
            _enqueue(
                db,
                purpose=PURPOSE_NOTIFICATION,
                recipient_email=subscription.email,
                subject=subject,
                body=body,
                idempotency_key=f"jami3ati:{notification.id}:{subscription.id}",
                correlation_id=notification.id,
                notification_id=notification.id,
                subscription_id=subscription.id,
                list_unsubscribe=header_unsubscribe,
            )
        )
    return created


def enqueue_moderator_report(
    db: Session,
    *,
    moderator_email: str,
    report_id: str,
    target_name: str,
    field_name: str,
    source_url: str,
    correction: str,
) -> NotificationDeliveryModel:
    subject, body = render_moderator_report(
        report_id=report_id,
        target_name=target_name,
        field_name=field_name,
        source_url=source_url,
        correction=correction,
    )
    return _enqueue(
        db,
        purpose=PURPOSE_MODERATOR,
        recipient_email=moderator_email,
        subject=subject,
        body=body,
        idempotency_key=f"jami3ati:report:{report_id}",
        correlation_id=report_id,
    )


# --------------------------------------------------------------------------
# Worker
# --------------------------------------------------------------------------


def process_pending_deliveries(
    db: Session,
    client: tira_outbound.TiraOutboundClient,
    *,
    max_attempts: int,
    batch_size: int,
    now: Optional[datetime] = None,
) -> dict:
    """Claim due rows, hand them to Tira, record what Tira said.

    `FOR UPDATE SKIP LOCKED` makes two workers safe to run at once: each row is
    claimed by exactly one of them. The row lock is held across the HTTP call
    on purpose — a shorter claim would need a `claimed_at` lease column, and
    for a single demo worker the tradeoff is not worth the extra state.
    """
    moment = now or utcnow()
    rows: Iterable[NotificationDeliveryModel] = (
        db.query(NotificationDeliveryModel)
        .filter(NotificationDeliveryModel.status == STATUS_PENDING)
        .filter(NotificationDeliveryModel.next_attempt_at <= moment)
        .order_by(NotificationDeliveryModel.next_attempt_at)
        .with_for_update(skip_locked=True)
        .limit(batch_size)
        .all()
    )

    summary = {
        "processed": 0,
        "accepted": 0,
        "failed": 0,
        "retried": 0,
        "dead": 0,
    }

    for row in rows:
        summary["processed"] += 1
        outcome = client.send(
            to=row.recipient_email,
            subject=row.subject,
            text=row.body,
            idempotency_key=row.idempotency_key,
            correlation_id=row.correlation_id,
            list_unsubscribe=row.list_unsubscribe,
            reply_to=row.reply_to,
        )
        row.attempts = (row.attempts or 0) + 1

        if outcome.kind == tira_outbound.ACCEPTED:
            row.status = STATUS_ACCEPTED
            row.tira_message_id = outcome.tira_message_id
            row.accepted_at = moment
            row.last_error = None
            summary["accepted"] += 1
            continue

        if outcome.kind == tira_outbound.REJECTED:
            row.status = STATUS_FAILED
            row.last_error = outcome.detail
            summary["failed"] += 1
            continue

        row.last_error = outcome.detail
        if row.attempts >= max_attempts:
            row.status = STATUS_DEAD
            summary["dead"] += 1
        else:
            row.next_attempt_at = moment + _backoff(row.attempts)
            summary["retried"] += 1

    db.commit()
    return summary
