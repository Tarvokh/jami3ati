"""Notification archive and the fan-out trigger.

Reading is public: a citizen must be able to check what was published, when,
and from which official URL, without an account. Writing needs the admin
token, and the token is compared in constant time.
"""

import logging
import uuid
from typing import List, Optional

from fastapi import APIRouter, Depends, Header, HTTPException, Query, Response
from sqlalchemy.orm import Session

from app.core.admin import require_admin
from app.core.clock import utcnow
from app.core.config import settings
from app.core.database import get_db
from app.models.notification import NotificationDeliveryModel, NotificationModel
from app.schemas.notification import (
    DeliveryResponse,
    NotificationCreate,
    NotificationResponse,
)
from app.services import deliveries

logger = logging.getLogger("jami3ati.notifications")

router = APIRouter(prefix="/notifications", tags=["Notifications"])


def _hmac_secret() -> str:
    secret = settings.JAMI3ATI_HMAC_SECRET.strip()
    if not secret:
        raise HTTPException(
            status_code=503,
            detail="JAMI3ATI_HMAC_SECRET غير مضبوط؛ الإرسال معطّل.",
        )
    return secret


@router.get("", response_model=List[NotificationResponse])
def list_notifications(
    institution_id: Optional[str] = Query(None),
    limit: int = Query(50, ge=1, le=200),
    offset: int = Query(0, ge=0),
    db: Session = Depends(get_db),
):
    query = db.query(NotificationModel)
    if institution_id:
        query = query.filter(NotificationModel.institution_id == institution_id)
    return (
        query.order_by(NotificationModel.created_at.desc())
        .offset(offset)
        .limit(limit)
        .all()
    )


@router.get("/{notification_id}", response_model=NotificationResponse)
def get_notification(notification_id: str, db: Session = Depends(get_db)):
    row = (
        db.query(NotificationModel)
        .filter(NotificationModel.id == notification_id)
        .first()
    )
    if row is None:
        raise HTTPException(status_code=404, detail="التنبيه غير موجود")
    return row


@router.post("", response_model=NotificationResponse, status_code=201)
def create_notification(
    payload: NotificationCreate,
    response: Response,
    x_jami3ati_admin: Optional[str] = Header(None, alias="X-Jami3ati-Admin"),
    db: Session = Depends(get_db),
):
    require_admin(x_jami3ati_admin)
    secret = _hmac_secret() if payload.dispatch else ""

    if payload.dedupe_key:
        existing = (
            db.query(NotificationModel)
            .filter(NotificationModel.dedupe_key == payload.dedupe_key)
            .first()
        )
        if existing is not None:
            response.status_code = 200
            return existing

    notification = NotificationModel(
        id=f"ntf-{uuid.uuid4().hex[:12]}",
        institution_id=payload.institution_id,
        kind=payload.kind,
        title_ar=payload.title_ar,
        body_ar=payload.body_ar,
        source_url=payload.source_url,
        verified_at=payload.verified_at or utcnow(),
        source_notification_id=payload.source_notification_id,
        dedupe_key=payload.dedupe_key,
    )
    db.add(notification)

    if payload.dispatch:
        created = deliveries.fan_out_notification(
            db,
            notification,
            base_url=settings.JAMI3ATI_PUBLIC_BASE_URL,
            hmac_secret=secret,
        )
        logger.info(
            "notification %s queued for %d active subscriptions",
            notification.id,
            len(created),
        )

    db.commit()
    db.refresh(notification)
    return notification


@router.get("/{notification_id}/deliveries", response_model=List[DeliveryResponse])
def list_deliveries(
    notification_id: str,
    x_jami3ati_admin: Optional[str] = Header(None, alias="X-Jami3ati-Admin"),
    db: Session = Depends(get_db),
):
    # Recipient addresses are personal data: admin-only, never public.
    require_admin(x_jami3ati_admin)
    return (
        db.query(NotificationDeliveryModel)
        .filter(NotificationDeliveryModel.notification_id == notification_id)
        .order_by(NotificationDeliveryModel.created_at)
        .all()
    )
