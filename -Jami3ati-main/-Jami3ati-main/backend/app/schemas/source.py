from datetime import datetime
from typing import Optional

from pydantic import BaseModel, field_validator


class SourceCreate(BaseModel):
    name_ar: str
    url: str
    institution_id: Optional[str] = None
    check_interval_minutes: int = 360
    is_active: bool = True
    is_official: bool = True

    @field_validator("name_ar")
    @classmethod
    def _name_not_blank(cls, value: str) -> str:
        value = value.strip()
        if not value:
            raise ValueError("must not be blank")
        return value

    @field_validator("url")
    @classmethod
    def _http_url(cls, value: str) -> str:
        value = value.strip()
        if not value.startswith(("http://", "https://")):
            raise ValueError("url must start with http:// or https://")
        return value

    @field_validator("check_interval_minutes")
    @classmethod
    def _sane_interval(cls, value: int) -> int:
        # 15 minutes is the floor: polling faster than that is rude to a public
        # server and does not make the alert meaningfully faster.
        if value < 15 or value > 10080:
            raise ValueError("check_interval_minutes must be between 15 and 10080")
        return value


class SourceResponse(BaseModel):
    id: str
    name_ar: str
    url: str
    institution_id: Optional[str]
    is_active: bool
    is_official: bool
    check_interval_minutes: int
    last_checked_at: Optional[datetime]
    last_changed_at: Optional[datetime]
    last_status: Optional[str]
    last_error: Optional[str]
    last_hash: Optional[str]
    created_at: datetime

    class Config:
        from_attributes = True


class PollResponse(BaseModel):
    source_id: str
    status: str
    fact_id: Optional[str] = None
    detail: str = ""
