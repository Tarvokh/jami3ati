import json
import os
from app.core.database import SessionLocal, Base, engine
from app.models.university import UniversityModel
from app.models.institution import InstitutionModel


def seed_database():
    Base.metadata.create_all(bind=engine)
    db = SessionLocal()

    seed_file = os.path.join(os.path.dirname(__file__), "demo_data.json")
    with open(seed_file, "r", encoding="utf-8") as f:
        data = json.load(f)

    # Seed Universities
    for u in data.get("universities", []):
        exists = db.query(UniversityModel).filter(UniversityModel.id == u["id"]).first()
        if not exists:
            db.add(UniversityModel(**u))

    db.commit()

    # Seed Institutions
    for inst in data.get("institutions", []):
        exists = db.query(InstitutionModel).filter(InstitutionModel.id == inst["id"]).first()
        if not exists:
            db.add(InstitutionModel(**inst))

    db.commit()
    db.close()
    print("✅ تم بنجاح حقن البيانات التجريبية والمحققة في قاعدة البيانات!")


if __name__ == "__main__":
    seed_database()
