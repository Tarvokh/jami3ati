import re
from datetime import datetime
from typing import Optional

from pydantic import BaseModel, field_validator

# Deliberately strict: one @, a dot in the domain, no whitespace. The real
# proof of deliverability is the confirmation email, not this regex.
_EMAIL_RE = re.compile(r"^[^@\s]+@[^@\s]+\.[A-Za-z]{2,}$")


class SubscriptionCreate(BaseModel):
    email: str
    institution_id: Optional[str] = None

    @field_validator("email")
    @classmethod
    def _normalise_email(cls, value: str) -> str:
        value = value.strip().lower()
        if len(value) > 254 or not _EMAIL_RE.match(value):
            raise ValueError("email is not a valid address")
        return value


class SubscriptionResponse(BaseModel):
    id: str
    email: str
    institution_id: Optional[str]
    status: str
    created_at: datetime

    class Config:
        from_attributes = True


class SubscriptionStatusResponse(BaseModel):
    id: str
    status: str
