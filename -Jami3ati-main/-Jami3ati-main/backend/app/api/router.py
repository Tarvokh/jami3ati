from fastapi import APIRouter
from app.api.routes import (
    health,
    institutions,
    notifications,
    reports,
    subscriptions,
    universities,
)

api_router = APIRouter()

api_router.include_router(health.router)
api_router.include_router(universities.router)
api_router.include_router(institutions.router)
api_router.include_router(reports.router)
api_router.include_router(subscriptions.router)
api_router.include_router(notifications.router)
