import logging
import uuid

from fastapi import APIRouter, Depends, Header, HTTPException, Query
from sqlalchemy.orm import Session
from typing import List, Optional

from app.core.admin import require_admin
from app.core.config import settings
from app.core.database import get_db
from app.models.report import InaccuracyReportModel
from app.schemas.report import InaccuracyReportResponse, InaccuracyReportCreate, ReportStatusUpdate
from app.services import deliveries

logger = logging.getLogger("jami3ati.reports")

router = APIRouter(prefix="/reports", tags=["Reports & Quality"])


@router.get("", response_model=List[InaccuracyReportResponse])
def get_reports(
    status: Optional[str] = Query(None, description="تصفية حسب الحالة (pending, approved, rejected)"),
    db: Session = Depends(get_db),
):
    query = db.query(InaccuracyReportModel)
    if status:
        query = query.filter(InaccuracyReportModel.status == status)
    return query.order_by(InaccuracyReportModel.created_at.desc()).all()


@router.post("", response_model=InaccuracyReportResponse, status_code=201)
def submit_report(payload: InaccuracyReportCreate, db: Session = Depends(get_db)):
    if not payload.source_proof_url or not payload.source_proof_url.startswith("http"):
        raise HTTPException(
            status_code=422,
            detail="يلزم تقديم رابط مصدر رسمي صالح يبدأ بـ http:// أو https:// لإثبات التصحيح",
        )

    report_id = f"rep-{uuid.uuid4().hex[:6]}"
    report = InaccuracyReportModel(
        id=report_id,
        status="pending",
        **payload.model_dump(),
    )
    db.add(report)
    db.commit()
    db.refresh(report)

    # Notify the moderator through the same outbox as every other message.
    # A failure here must never lose the report that was just accepted, so the
    # enqueue is best-effort and logged rather than raised.
    moderator = settings.MODERATOR_EMAIL.strip()
    if moderator:
        try:
            deliveries.enqueue_moderator_report(
                db,
                moderator_email=moderator,
                report_id=report.id,
                target_name=report.target_name,
                field_name=report.field_name,
                source_url=report.source_proof_url,
                correction=report.proposed_correction,
            )
            db.commit()
        except Exception:  # noqa: BLE001
            db.rollback()
            logger.exception("moderator notification enqueue failed for %s", report_id)

    return report


@router.patch("/{report_id}/status", response_model=InaccuracyReportResponse)
def update_report_status(
    report_id: str,
    payload: ReportStatusUpdate,
    x_jami3ati_admin: Optional[str] = Header(None, alias="X-Jami3ati-Admin"),
    db: Session = Depends(get_db),
):
    # Moderation is an administrative act: it changes what the public archive
    # may later say about an institution. Until this guard existed, any caller
    # could flip a report to `approved` without a token. Same constant-time
    # check as the notification write surface; an unset token fails closed.
    require_admin(x_jami3ati_admin)

    report = db.query(InaccuracyReportModel).filter(InaccuracyReportModel.id == report_id).first()
    if not report:
        raise HTTPException(status_code=404, detail="البلاغ غير موجود")

    report.status = payload.status
    db.commit()
    db.refresh(report)
    return report
