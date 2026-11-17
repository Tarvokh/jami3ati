"""Admin-token guard shared by every write endpoint that changes public state.

The token is compared in constant time. An unset token disables the endpoint
with 503 instead of accepting an empty header, so a misconfigured deployment
fails closed: no token means no moderation, never "everyone is an admin".
"""

import hmac
from typing import Optional

from fastapi import HTTPException

from app.core.config import settings


def require_admin(token: Optional[str]) -> None:
    expected = settings.JAMI3ATI_ADMIN_TOKEN.strip()
    if not expected:
        raise HTTPException(
            status_code=503,
            detail="JAMI3ATI_ADMIN_TOKEN غير مضبوط؛ النشر معطّل.",
        )
    if not token or not hmac.compare_digest(expected, token):
        raise HTTPException(status_code=401, detail="رمز الإدارة غير صالح.")
