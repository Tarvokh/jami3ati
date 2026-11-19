"""The fact archive and the human review gate.

Public reads return approved facts only. A pending fact is visible to the
admin token holder, because that is the reviewer's queue; nobody else can see
an unreviewed change, so a false positive cannot leak into the public archive.
"""

import logging
from typing import List, Optional

from fastapi import APIRouter, Depends, Header, HTTPException, Query
from sqlalchemy.orm import Session

from app.core.admin import require_admin
from app.core.config import settings
from app.core.database import get_db
from app.models.fact import FactModel
from app.schemas.fact import (
    FactApprove,
    FactDetailResponse,
    FactReject,
    FactResponse,
)
from app.schemas.notification import NotificationResponse
from app.services import poller

logger = logging.getLogger("jami3ati.facts")

router = APIRouter(prefix="/facts", tags=["Observatory"])


@router.get("", response_model=List[FactResponse])
def list_facts(
    status: Optional[str] = Query(None, description="approved (عام) أو pending/rejected للإدارة"),
    limit: int = Query(50, ge=1, le=200),
    offset: int = Query(0, ge=0),
    x_jami3ati_admin: Optional[str] = Header(None, alias="X-Jami3ati-Admin"),
    db: Session = Depends(get_db),
):
    # Anything other than `approved` is reviewer-only.
    if status and status != "approved":
        require_admin(x_jami3ati_admin)

    query = db.query(FactModel)
    if status:
        query = query.filter(FactModel.status == status)
    else:
        query = query.filter(FactModel.status == "approved")

    return (
        query.order_by(FactModel.verified_at.desc())
        .offset(offset)
        .limit(limit)
        .all()
    )


@router.get("/{fact_id}", response_model=FactDetailResponse)
def get_fact(
    fact_id: str,
    x_jami3ati_admin: Optional[str] = Header(None, alias="X-Jami3ati-Admin"),
    db: Session = Depends(get_db),
):
    fact = db.query(FactModel).filter(FactModel.id == fact_id).first()
    if fact is None:
        raise HTTPException(status_code=404, detail="الواقعة غير موجودة")
    if fact.status != "approved":
        require_admin(x_jami3ati_admin)
    return fact


@router.post("/{fact_id}/approve", response_model=NotificationResponse)
def approve(
    fact_id: str,
    payload: FactApprove,
    x_jami3ati_admin: Optional[str] = Header(None, alias="X-Jami3ati-Admin"),
    db: Session = Depends(get_db),
):
    require_admin(x_jami3ati_admin)

    fact = db.query(FactModel).filter(FactModel.id == fact_id).first()
    if fact is None:
        raise HTTPException(status_code=404, detail="الواقعة غير موجودة")

    secret = settings.JAMI3ATI_HMAC_SECRET.strip()
    if payload.dispatch and not secret:
        # Sending without a verifiable unsubscribe link is not allowed.
        raise HTTPException(
            status_code=503,
            detail="JAMI3ATI_HMAC_SECRET غير مضبوط؛ لا يمكن إرسال التنبيه.",
        )

    try:
        notification = poller.approve_fact(
            db,
            fact,
            base_url=settings.JAMI3ATI_PUBLIC_BASE_URL,
            hmac_secret=secret,
            dispatch=payload.dispatch,
        )
    except ValueError as exc:
        raise HTTPException(status_code=409, detail=str(exc)) from exc

    logger.info(
        "fact %s approved as notification %s (dispatch=%s)",
        fact.id,
        notification.id,
        payload.dispatch,
    )
    return notification


@router.post("/{fact_id}/reject", response_model=FactResponse)
def reject(
    fact_id: str,
    payload: FactReject,
    x_jami3ati_admin: Optional[str] = Header(None, alias="X-Jami3ati-Admin"),
    db: Session = Depends(get_db),
):
    require_admin(x_jami3ati_admin)

    fact = db.query(FactModel).filter(FactModel.id == fact_id).first()
    if fact is None:
        raise HTTPException(status_code=404, detail="الواقعة غير موجودة")

    try:
        return poller.reject_fact(db, fact, note=payload.note)
    except ValueError as exc:
        raise HTTPException(status_code=409, detail=str(exc)) from exc
