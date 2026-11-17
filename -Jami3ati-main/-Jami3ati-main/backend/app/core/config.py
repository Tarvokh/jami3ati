from pydantic_settings import BaseSettings
from typing import List


class Settings(BaseSettings):
    PROJECT_NAME: str = "جامعتي | Jami3ati API"
    VERSION: str = "1.0.0-hackathon-mvp"
    API_V1_STR: str = "/api/v1"
    DATABASE_URL: str = "postgresql://jami3ati:jami3ati_secret_dev@localhost:5432/jami3ati_db"
    CORS_ORIGINS: List[str] = ["http://localhost:3000", "http://127.0.0.1:3000", "*"]

    # --- Jami3ati delivery layer (Loop 1) ---
    # Public base URL used to build confirmation and unsubscribe links. It must
    # be https:// for the List-Unsubscribe header to be emitted (Tira accepts
    # only <https://...> or <mailto:...>); over http the body link still works.
    JAMI3ATI_PUBLIC_BASE_URL: str = "http://localhost:8000"
    # Required. Without it subscriptions answer 503: an unsubscribe link that
    # cannot be verified is not a valid consent record.
    JAMI3ATI_HMAC_SECRET: str = ""
    # Required for POST /notifications. Empty means the write surface is off.
    JAMI3ATI_ADMIN_TOKEN: str = ""
    # Where report notifications go. Empty means no moderator email is queued
    # (the report itself is still accepted).
    MODERATOR_EMAIL: str = ""

    # --- Tira machine send contract ---
    TIRA_OUTBOUND_URL: str = "http://127.0.0.1:8080/api/v1/outbound"
    TIRA_OUTBOUND_TOKEN: str = ""
    TIRA_MAILBOX_ID: str = ""
    TIRA_REPLY_TO: str = ""

    # --- Delivery worker ---
    DELIVERY_POLL_SECONDS: int = 5
    DELIVERY_BATCH_SIZE: int = 25
    DELIVERY_MAX_ATTEMPTS: int = 3
    DELIVERY_HTTP_TIMEOUT_SECONDS: int = 10
    CONFIRM_TOKEN_TTL_HOURS: int = 48

    class Config:
        env_file = ".env"
        case_sensitive = True


settings = Settings()
