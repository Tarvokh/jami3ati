from sqlalchemy import Column, String, DateTime, Text
from datetime import datetime
from app.core.database import Base


class InaccuracyReportModel(Base):
    __tablename__ = "inaccuracy_reports"

    id = Column(String, primary_key=True, index=True)
    target_id = Column(String, index=True, nullable=False)
    target_name = Column(String, nullable=False)
    field_name = Column(String, nullable=False)
    current_value = Column(String, nullable=True)
    proposed_correction = Column(Text, nullable=False)
    source_proof_url = Column(String, nullable=False)
    user_notes = Column(Text, nullable=True)
    reporter_email = Column(String, nullable=True)
    status = Column(String, default="pending", nullable=False)
    created_at = Column(DateTime, default=datetime.utcnow)
