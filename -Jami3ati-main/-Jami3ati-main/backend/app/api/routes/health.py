from fastapi import APIRouter
from app.core.config import settings

router = APIRouter(tags=["Health"])


@router.get("/health")
def health_check():
    return {
        "status": "healthy",
        "service": settings.PROJECT_NAME,
        "version": settings.VERSION,
        "principles": {
            "open_data_first": True,
            "zero_hallucination": True,
            "no_personal_data_collection": True,
        },
    }
