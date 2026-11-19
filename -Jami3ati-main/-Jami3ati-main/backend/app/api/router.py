from fastapi import APIRouter
from app.api.routes import (
    facts,
    health,
    institutions,
    notifications,
    reports,
    sources,
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
api_router.include_router(sources.router)
api_router.include_router(facts.router)
