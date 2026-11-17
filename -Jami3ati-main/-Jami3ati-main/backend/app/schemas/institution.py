from pydantic import BaseModel
from typing import Optional, List
from datetime import datetime


class InstitutionBase(BaseModel):
    code: str
    university_id: str
    name_ar: str
    name_fr: str
    type: str
    type_label_ar: str
    type_label_fr: str
    city_ar: str
    city_fr: str
    campus_ar: Optional[str] = None
    campus_fr: Optional[str] = None
    website_url: str
    phone: Optional[str] = None
    email: Optional[str] = None
    source_url: str
    source_name: str
    verified_at: str
    data_status: str = "verified"
    programs_summary_ar: List[str] = []
    programs_summary_fr: List[str] = []
    access_mode_ar: Optional[str] = None
    access_mode_fr: Optional[str] = None
    image_url: Optional[str] = None


class InstitutionCreate(InstitutionBase):
    id: str


class InstitutionResponse(InstitutionBase):
    id: str
    created_at: Optional[datetime] = None

    class Config:
        from_attributes = True
