from fastapi import APIRouter, Depends, HTTPException, Query
from sqlalchemy.orm import Session
from typing import List, Optional

from app.core.database import get_db
from app.models.institution import InstitutionModel
from app.schemas.institution import InstitutionResponse, InstitutionCreate

router = APIRouter(prefix="/institutions", tags=["Institutions"])


@router.get("", response_model=List[InstitutionResponse])
def get_institutions(
    university_id: Optional[str] = Query(None, description="تصفية حسب الجامعة التابعة لها"),
    city: Optional[str] = Query(None, description="تصفية حسب المدينة"),
    type: Optional[str] = Query(None, description="نوع المؤسسة (engineering_school, faculty, etc.)"),
    status: Optional[str] = Query(None, description="حالة التوثيق (verified, needs_review, demo)"),
    search: Optional[str] = Query(None, description="البحث بالنص"),
    db: Session = Depends(get_db),
):
    query = db.query(InstitutionModel)

    if university_id:
        query = query.filter(InstitutionModel.university_id == university_id)
    if city:
        query = query.filter(
            (InstitutionModel.city_ar == city) | (InstitutionModel.city_fr == city)
        )
    if type:
        query = query.filter(InstitutionModel.type == type)
    if status:
        query = query.filter(InstitutionModel.data_status == status)
    if search:
        search_fmt = f"%{search.lower()}%"
        query = query.filter(
            (InstitutionModel.name_ar.ilike(search_fmt))
            | (InstitutionModel.name_fr.ilike(search_fmt))
            | (InstitutionModel.code.ilike(search_fmt))
        )

    return query.all()


@router.get("/{inst_id}", response_model=InstitutionResponse)
def get_institution_by_id(inst_id: str, db: Session = Depends(get_db)):
    inst = db.query(InstitutionModel).filter(
        (InstitutionModel.id == inst_id) | (InstitutionModel.code == inst_id.upper())
    ).first()
    if not inst:
        raise HTTPException(status_code=404, detail="المؤسسة أو الكلية غير موجودة")
    return inst


@router.post("", response_model=InstitutionResponse, status_code=201)
def create_institution(payload: InstitutionCreate, db: Session = Depends(get_db)):
    existing = db.query(InstitutionModel).filter(InstitutionModel.code == payload.code).first()
    if existing:
        raise HTTPException(status_code=400, detail="رمز المؤسسة مسجل مسبقاً")

    inst = InstitutionModel(**payload.model_dump())
    db.add(inst)
    db.commit()
    db.refresh(inst)
    return inst
