from datetime import datetime
from typing import Optional

from pydantic import BaseModel


class FactResponse(BaseModel):
    """A fact as listed. The full snapshot is only on the detail/proof view."""

    id: str
    source_id: str
    institution_id: Optional[str]
    source_url: str
    source_name: str
    title_ar: str
    body_ar: str
    diff_text: str
    snapshot_hash: str
    previous_hash: Optional[str]
    status: str
    notification_id: Optional[str]
    review_note: Optional[str]
    verified_at: datetime
    created_at: datetime
    approved_at: Optional[datetime]

    class Config:
        from_attributes = True


class FactDetailResponse(FactResponse):
    snapshot_text: str


class FactApprove(BaseModel):
    # A reviewer may record the fact without emailing it, e.g. when the change
    # is real but not relevant to subscribers.
    dispatch: bool = True


class FactReject(BaseModel):
    note: str = ""
