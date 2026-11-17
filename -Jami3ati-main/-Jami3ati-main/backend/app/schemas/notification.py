from datetime import datetime
from typing import Optional

from pydantic import BaseModel, field_validator


class NotificationCreate(BaseModel):
    institution_id: Optional[str] = None
    kind: str = "official_change"
    title_ar: str
    body_ar: str
    source_url: str
    verified_at: Optional[datetime] = None
    source_notification_id: Optional[str] = None
    dedupe_key: Optional[str] = None
    # `dispatch=False` records the fact without fanning it out — useful while
    # a source is still under review.
    dispatch: bool = True

    @field_validator("title_ar", "body_ar")
    @classmethod
    def _not_blank(cls, value: str) -> str:
        value = value.strip()
        if not value:
            raise ValueError("must not be blank")
        return value

    @field_validator("source_url")
    @classmethod
    def _official_source(cls, value: str) -> str:
        value = value.strip()
        if not value.startswith(("http://", "https://")):
            raise ValueError("source_url must start with http:// or https://")
        return value


class NotificationResponse(BaseModel):
    id: str
    institution_id: Optional[str]
    kind: str
    title_ar: str
    body_ar: str
    source_url: str
    verified_at: Optional[datetime]
    source_notification_id: Optional[str]
    created_at: datetime

    class Config:
        from_attributes = True


class DeliveryResponse(BaseModel):
    id: str
    notification_id: Optional[str]
    purpose: str
    recipient_email: str
    status: str
    attempts: int
    tira_message_id: Optional[str]
    last_error: Optional[str]
    created_at: datetime

    class Config:
        from_attributes = True
