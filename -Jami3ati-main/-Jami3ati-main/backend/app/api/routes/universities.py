from fastapi import APIRouter, Depends, HTTPException, Query
from sqlalchemy.orm import Session
from typing import List, Optional

from app.core.database import get_db
from app.models.university import UniversityModel
from app.schemas.university import UniversityResponse, UniversityCreate

router = APIRouter(prefix="/universities", tags=["Universities"])


@router.get("", response_model=List[UniversityResponse])
def get_universities(
    city: Optional[str] = Query(None, description="تصفية حسب المدينة"),
    status: Optional[str] = Query(None, description="حالة التوثيق (verified, needs_review, demo)"),
    search: Optional[str] = Query(None, description="البحث بالاسم أو الرمز"),
    db: Session = Depends(get_db),
):
    query = db.query(UniversityModel)

    if city:
        query = query.filter(
            (UniversityModel.city_ar == city) | (UniversityModel.city_fr == city)
        )
    if status:
        query = query.filter(UniversityModel.data_status == status)
    if search:
        search_fmt = f"%{search.lower()}%"
        query = query.filter(
            (UniversityModel.name_ar.ilike(search_fmt))
            | (UniversityModel.name_fr.ilike(search_fmt))
            | (UniversityModel.code.ilike(search_fmt))
        )

    return query.all()


@router.get("/{univ_id}", response_model=UniversityResponse)
def get_university_by_id(univ_id: str, db: Session = Depends(get_db)):
    univ = db.query(UniversityModel).filter(
        (UniversityModel.id == univ_id) | (UniversityModel.code == univ_id.upper())
    ).first()
    if not univ:
        raise HTTPException(status_code=404, detail="المؤسسة الجامعية غير موجودة")
    return univ


@router.post("", response_model=UniversityResponse, status_code=201)
def create_university(payload: UniversityCreate, db: Session = Depends(get_db)):
    existing = db.query(UniversityModel).filter(UniversityModel.code == payload.code).first()
    if existing:
        raise HTTPException(status_code=400, detail="رمز الجامعة مسجل مسبقاً")

    univ = UniversityModel(**payload.model_dump())
    db.add(univ)
    db.commit()
    db.refresh(univ)
    return univ
