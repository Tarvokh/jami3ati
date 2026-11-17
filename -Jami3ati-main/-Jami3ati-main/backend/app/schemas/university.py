from pydantic import BaseModel, HttpUrl
from typing import Optional, List
from datetime import datetime


class UniversityBase(BaseModel):
    code: str
    name_ar: str
    name_fr: str
    acronym: str
    city_ar: str
    city_fr: str
    region_ar: Optional[str] = None
    region_fr: Optional[str] = None
    established_year: Optional[int] = None
    website_url: str
    portal_student_url: Optional[str] = None
    description_ar: Optional[str] = None
    description_fr: Optional[str] = None
    image_url: Optional[str] = None
    phone: Optional[str] = None
    email: Optional[str] = None
    address_ar: Optional[str] = None
    address_fr: Optional[str] = None
    source_url: str
    source_name: str
    verified_at: str
    data_status: str = "verified"


class UniversityCreate(UniversityBase):
    id: str


class UniversityResponse(UniversityBase):
    id: str
    created_at: Optional[datetime] = None

    class Config:
        from_attributes = True
