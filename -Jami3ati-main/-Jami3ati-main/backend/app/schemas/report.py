from pydantic import BaseModel
from typing import Optional
from datetime import datetime


class InaccuracyReportCreate(BaseModel):
    target_id: str
    target_name: str
    field_name: str
    current_value: Optional[str] = None
    proposed_correction: str
    source_proof_url: str
    user_notes: Optional[str] = None
    reporter_email: Optional[str] = None


class InaccuracyReportResponse(InaccuracyReportCreate):
    id: str
    status: str
    created_at: datetime

    class Config:
        from_attributes = True


class ReportStatusUpdate(BaseModel):
    status: str
