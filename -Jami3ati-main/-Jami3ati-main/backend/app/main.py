import logging
import threading
from contextlib import asynccontextmanager

from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware

from app.api.router import api_router
from app.api.routes import proof
from app.core.config import settings
from app.core.database import Base, SessionLocal, engine

# Import every model module before create_all: metadata is populated by import
# side effects, and a model that is never imported never becomes a table.
from app.models import (  # noqa: F401
    fact,
    institution,
    notification,
    report,
    source,
    subscription,
    university,
)

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


def _poller_loop(stop: threading.Event) -> None:
    """Poll due sources until the process stops.

    Disabled unless POLLER_ENABLED is true. A pass never raises: a source that
    is down is recorded on its row, and the next tick tries again.
    """
    from app.services import poller

    logger.warning(
        "poller started: every %ss, user agent %s",
        settings.POLLER_INTERVAL_SECONDS,
        settings.POLLER_USER_AGENT,
    )
    while not stop.is_set():
        try:
            with SessionLocal() as db:
                outcomes = poller.poll_due_sources(
                    db,
                    user_agent=settings.POLLER_USER_AGENT,
                    limit=settings.POLLER_MAX_SOURCES_PER_PASS,
                )
            changed = [o for o in outcomes if o.status == "changed"]
            if changed:
                logger.warning(
                    "poller pass found %d change(s): %s",
                    len(changed),
                    [o.fact_id for o in changed],
                )
        except Exception:  # noqa: BLE001 - the loop must survive one bad pass
            logger.exception("poller pass failed; retrying next tick")
        stop.wait(settings.POLLER_INTERVAL_SECONDS)


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

    poller_thread = None
    if settings.POLLER_ENABLED:
        poller_thread = threading.Thread(
            target=_poller_loop,
            args=(stop,),
            name="jami3ati-poller",
            daemon=True,
        )
        poller_thread.start()
    else:
        logger.info("poller disabled: set POLLER_ENABLED=true to enable")

    try:
        yield
    finally:
        stop.set()
        for worker in (thread, poller_thread):
            if worker is not None:
                worker.join(timeout=5)


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
# The proof page lives at the root, not under /api/v1: it is a public document
# a student can open from an email, not a machine endpoint.
app.include_router(proof.router)


@app.get("/")
def root():
    return {
        "message": "مرحبًا بكم في واجهة برمجة تطبيقات جامعتي | Jami3ati API",
        "docs": f"{settings.API_V1_STR}/docs",
        "health": f"{settings.API_V1_STR}/health",
    }
