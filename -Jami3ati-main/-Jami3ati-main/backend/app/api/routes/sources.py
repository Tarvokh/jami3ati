"""The source registry: what the observatory watches, and the manual trigger.

Reading the registry is public — a citizen should be able to see which pages
are monitored. Adding a source and polling it are admin actions, because they
change what the archive may later claim.
"""

import logging
import uuid
from typing import List, Optional

from fastapi import APIRouter, Depends, Header, HTTPException
from sqlalchemy.orm import Session

from app.core.admin import require_admin
from app.core.database import get_db
from app.models.source import SourceModel
from app.schemas.source import PollResponse, SourceCreate, SourceResponse
from app.services import poller

logger = logging.getLogger("jami3ati.sources")

router = APIRouter(prefix="/sources", tags=["Observatory"])


@router.get("", response_model=List[SourceResponse])
def list_sources(db: Session = Depends(get_db)):
    return db.query(SourceModel).order_by(SourceModel.created_at.desc()).all()


@router.post("", response_model=SourceResponse, status_code=201)
def create_source(
    payload: SourceCreate,
    x_jami3ati_admin: Optional[str] = Header(None, alias="X-Jami3ati-Admin"),
    db: Session = Depends(get_db),
):
    require_admin(x_jami3ati_admin)

    existing = (
        db.query(SourceModel).filter(SourceModel.url == payload.url).first()
    )
    if existing is not None:
        raise HTTPException(
            status_code=409,
            detail=f"هذا المصدر مسجَّل مسبقًا بالمعرّف {existing.id}",
        )

    source = SourceModel(
        id=f"src-{uuid.uuid4().hex[:10]}",
        **payload.model_dump(),
    )
    db.add(source)
    db.commit()
    db.refresh(source)
    logger.info("source %s registered for %s", source.id, source.url)
    return source


@router.post("/{source_id}/poll", response_model=PollResponse)
def poll_now(
    source_id: str,
    x_jami3ati_admin: Optional[str] = Header(None, alias="X-Jami3ati-Admin"),
    db: Session = Depends(get_db),
):
    require_admin(x_jami3ati_admin)

    source = db.query(SourceModel).filter(SourceModel.id == source_id).first()
    if source is None:
        raise HTTPException(status_code=404, detail="المصدر غير موجود")

    outcome = poller.poll_source(db, source)
    return PollResponse(
        source_id=outcome.source_id,
        status=outcome.status,
        fact_id=outcome.fact_id,
        detail=outcome.detail,
    )
