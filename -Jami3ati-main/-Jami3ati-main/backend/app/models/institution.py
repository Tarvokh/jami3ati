from sqlalchemy import Column, String, ForeignKey, DateTime, JSON
from sqlalchemy.orm import relationship
from datetime import datetime
from app.core.database import Base


class InstitutionModel(Base):
    __tablename__ = "institutions"

    id = Column(String, primary_key=True, index=True)
    code = Column(String, unique=True, index=True, nullable=False)
    university_id = Column(String, ForeignKey("universities.id"), nullable=False, index=True)
    name_ar = Column(String, nullable=False)
    name_fr = Column(String, nullable=False)
    type = Column(String, index=True, nullable=False)
    type_label_ar = Column(String, nullable=False)
    type_label_fr = Column(String, nullable=False)
    city_ar = Column(String, index=True, nullable=False)
    city_fr = Column(String, index=True, nullable=False)
    campus_ar = Column(String, nullable=True)
    campus_fr = Column(String, nullable=True)
    website_url = Column(String, nullable=False)
    phone = Column(String, nullable=True)
    email = Column(String, nullable=True)
    source_url = Column(String, nullable=False)
    source_name = Column(String, nullable=False)
    verified_at = Column(String, nullable=False)
    data_status = Column(String, default="verified", nullable=False)
    programs_summary_ar = Column(JSON, default=list)
    programs_summary_fr = Column(JSON, default=list)
    access_mode_ar = Column(String, nullable=True)
    access_mode_fr = Column(String, nullable=True)
    image_url = Column(String, nullable=True)
    created_at = Column(DateTime, default=datetime.utcnow)

    university = relationship("UniversityModel", back_populates="institutions")
