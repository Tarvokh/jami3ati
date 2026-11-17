import logging
import threading
from contextlib import asynccontextmanager

from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware

from app.api.router import api_router
from app.core.config import settings
from app.core.database import Base, SessionLocal, engine

# Import every model module before create_all: metadata is populated by import
# side effects, and a model that is never imported never becomes a table.
from app.models import institution, notification, report, subscription, university  # noqa: F401

logger = logging.getLogger("jami3ati.deliveries")

# Create tables in dev if not already created
Base.metadata.create_all(bind=engine)


def _delivery_loop(stop: threading.Event) -> None:
    """Drain the outbox until the process stops.

    Disabled entirely unless both TIRA_OUTBOUND_TOKEN and TIRA_MAILBOX_ID are
    set, so a demo run without credentials cannot silently claim to send.
    """
    from app.services import deliveries
    from app.services.tira_outbound import TiraNotConfigured, TiraOutboundClient

    try:
        client = TiraOutboundClient(
            settings.TIRA_OUTBOUND_URL,
            settings.TIRA_OUTBOUND_TOKEN,
            settings.TIRA_MAILBOX_ID,
            timeout_seconds=settings.DELIVERY_HTTP_TIMEOUT_SECONDS,
        )
    except TiraNotConfigured as exc:
        logger.error("delivery worker disabled: %s", exc)
        return

    logger.warning(
        "delivery worker started: %s (mailbox %s)",
        settings.TIRA_OUTBOUND_URL,
        settings.TIRA_MAILBOX_ID,
    )
    while not stop.is_set():
        try:
            with SessionLocal() as db:
                summary = deliveries.process_pending_deliveries(
                    db,
                    client,
                    max_attempts=settings.DELIVERY_MAX_ATTEMPTS,
                    batch_size=settings.DELIVERY_BATCH_SIZE,
                )
            if summary["processed"]:
                logger.info("delivery pass: %s", summary)
        except Exception:  # noqa: BLE001 - the loop must survive one bad pass
            logger.exception("delivery pass failed; retrying next tick")
        stop.wait(settings.DELIVERY_POLL_SECONDS)


@asynccontextmanager
async def lifespan(_: FastAPI):
    stop = threading.Event()
    thread = None
    if settings.TIRA_OUTBOUND_TOKEN and settings.TIRA_MAILBOX_ID:
        thread = threading.Thread(
            target=_delivery_loop,
            args=(stop,),
            name="jami3ati-deliveries",
            daemon=True,
        )
        thread.start()
    else:
        logger.warning(
            "delivery worker disabled: TIRA_OUTBOUND_TOKEN / TIRA_MAILBOX_ID unset"
        )
    try:
        yield
    finally:
        stop.set()
        if thread is not None:
            thread.join(timeout=5)


app = FastAPI(
    title=settings.PROJECT_NAME,
    version=settings.VERSION,
    description="""
    ## دليل الخدمات والمعلومات الجامعية بالمغرب (جامعتي | Jami3ati)
    API موثق تلقائيًا يعتمد على البيانات المفتوحة والمصادر الرسمية المنشورة.
    
    ### مبادئ البيانات:
    - **أولوية البيانات المفتوحة**: بيانات data.gov.ma وبوابات الجامعات.
    - **صفر تخمين**: عدم توليد أي أرقام أو إيميلات غير موثقة.
    - **توثيق المصدر وتاريخ المراجعة**: لكل سجل `source_url` و`source_name` و`verified_at`.
    - **حالات التوثيق**: `verified` (موثق)، `needs_review` (قيد المراجعة)، `demo` (تجريبي).
    """,
    openapi_url=f"{settings.API_V1_STR}/openapi.json",
    docs_url=f"{settings.API_V1_STR}/docs",
    redoc_url=f"{settings.API_V1_STR}/redoc",
    lifespan=lifespan,
)

# CORS setup
app.add_middleware(
    CORSMiddleware,
    allow_origins=settings.CORS_ORIGINS,
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

app.include_router(api_router, prefix=settings.API_V1_STR)


@app.get("/")
def root():
    return {
        "message": "مرحبًا بكم في واجهة برمجة تطبيقات جامعتي | Jami3ati API",
        "docs": f"{settings.API_V1_STR}/docs",
        "health": f"{settings.API_V1_STR}/health",
    }
