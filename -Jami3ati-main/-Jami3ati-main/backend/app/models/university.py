from sqlalchemy import Column, String, Integer, DateTime
from sqlalchemy.orm import relationship
from datetime import datetime
from app.core.database import Base


class UniversityModel(Base):
    __tablename__ = "universities"

    id = Column(String, primary_key=True, index=True)
    code = Column(String, unique=True, index=True, nullable=False)
    name_ar = Column(String, nullable=False)
    name_fr = Column(String, nullable=False)
    acronym = Column(String, nullable=False)
    city_ar = Column(String, index=True, nullable=False)
    city_fr = Column(String, index=True, nullable=False)
    region_ar = Column(String, nullable=True)
    region_fr = Column(String, nullable=True)
    established_year = Column(Integer, nullable=True)
    website_url = Column(String, nullable=False)
    portal_student_url = Column(String, nullable=True)
    description_ar = Column(String, nullable=True)
    description_fr = Column(String, nullable=True)
    image_url = Column(String, nullable=True)
    phone = Column(String, nullable=True)
    email = Column(String, nullable=True)
    address_ar = Column(String, nullable=True)
    address_fr = Column(String, nullable=True)
    source_url = Column(String, nullable=False)
    source_name = Column(String, nullable=False)
    verified_at = Column(String, nullable=False)
    data_status = Column(String, default="verified", nullable=False)
    created_at = Column(DateTime, default=datetime.utcnow)

    institutions = relationship("InstitutionModel", back_populates="university", cascade="all, delete-orphan")
