"""Test bootstrap.

The settings singleton is built at import time, so the environment must be
complete *before* `app.main` is imported. pytest imports `conftest.py` before
any test module, which is why this file starts with plain `os.environ` calls.

The database guard is deliberate: the autouse fixture drops every table, so
pointing `DATABASE_URL` at anything whose name does not contain "test" is a
hard error rather than a surprise data loss.
"""

import os

TEST_DATABASE_URL = "postgresql://postgres@127.0.0.1:55432/jami3ati_test"

os.environ.setdefault("DATABASE_URL", TEST_DATABASE_URL)
if "test" not in os.environ["DATABASE_URL"].lower():
    raise RuntimeError(
        "refusing to run tests: DATABASE_URL does not look like a test database"
    )

os.environ.setdefault("JAMI3ATI_HMAC_SECRET", "test-hmac-secret-not-for-production")
os.environ.setdefault("JAMI3ATI_ADMIN_TOKEN", "test-admin-token")
os.environ.setdefault("JAMI3ATI_PUBLIC_BASE_URL", "http://testserver")
os.environ.setdefault("MODERATOR_EMAIL", "moderator@tira.test")
os.environ.setdefault("TIRA_OUTBOUND_URL", "http://tira.invalid/api/v1/outbound")
os.environ.setdefault("TIRA_OUTBOUND_TOKEN", "")
os.environ.setdefault("TIRA_MAILBOX_ID", "")

import pytest  # noqa: E402
from fastapi.testclient import TestClient  # noqa: E402

from app.core.database import Base, SessionLocal, engine  # noqa: E402
from app.main import app  # noqa: E402
from app.services import tira_outbound  # noqa: E402
from app.services.tira_outbound import OutboundOutcome  # noqa: E402


@pytest.fixture(autouse=True)
def clean_database():
    Base.metadata.drop_all(bind=engine)
    Base.metadata.create_all(bind=engine)
    yield


@pytest.fixture
def client():
    return TestClient(app)


@pytest.fixture
def db():
    session = SessionLocal()
    try:
        yield session
    finally:
        session.close()


class FakeTira:
    """Records every send and answers with a scripted outcome.

    No network call is made: the worker's status mapping is what these tests
    pin, and a fake is the only way to prove the mapping without a live relay.
    """

    def __init__(self, outcomes=None):
        self.calls = []
        self.outcomes = list(outcomes or [])
        self.default = OutboundOutcome(
            tira_outbound.ACCEPTED,
            "accepted",
            "00000000-0000-4000-8000-000000000001",
        )

    def send(self, **kwargs):
        self.calls.append(kwargs)
        if self.outcomes:
            return self.outcomes.pop(0)
        return self.default


@pytest.fixture
def fake_tira():
    return FakeTira()
