import uuid
from datetime import datetime

from app.core.clock import utcnow
from app.core.config import settings
from app.models.notification import NotificationDeliveryModel, NotificationModel
from app.models.subscription import SubscriptionModel
from app.services import deliveries, tira_outbound
from app.services.tira_outbound import OutboundOutcome


def _make_subscription(db, email, status="active", institution_id=None):
    row = SubscriptionModel(
        id=f"sub-{uuid.uuid4().hex[:12]}",
        email=email,
        status=status,
        institution_id=institution_id,
        confirmed_at=utcnow() if status == "active" else None,
    )
    db.add(row)
    db.commit()
    db.refresh(row)
    return row


def _make_notification(db, institution_id=None, title="فتح الترشيح"):
    row = NotificationModel(
        id=f"ntf-{uuid.uuid4().hex[:12]}",
        institution_id=institution_id,
        kind="official_change",
        title_ar=title,
        body_ar="فُتح باب الترشيح حتى 30 نوفمبر 2026.",
        source_url="https://www.ensias.ma/avis",
        verified_at=utcnow(),
    )
    db.add(row)
    db.commit()
    db.refresh(row)
    return row


def _fan_out(db, notification):
    return deliveries.fan_out_notification(
        db,
        notification,
        base_url=settings.JAMI3ATI_PUBLIC_BASE_URL,
        hmac_secret=settings.JAMI3ATI_HMAC_SECRET,
    )


def test_accepted_maps_to_accepted_status(db, fake_tira):
    _make_subscription(db, "a@uit.ac.ma")
    notification = _make_notification(db)
    _fan_out(db, notification)
    db.commit()

    summary = deliveries.process_pending_deliveries(
        db, fake_tira, max_attempts=3, batch_size=10
    )
    assert summary == {
        "processed": 1,
        "accepted": 1,
        "failed": 0,
        "retried": 0,
        "dead": 0,
    }

    db.expire_all()
    row = db.query(NotificationDeliveryModel).one()
    assert row.status == "accepted"
    assert row.tira_message_id == "00000000-0000-4000-8000-000000000001"
    assert row.accepted_at is not None
    assert row.last_error is None
    assert fake_tira.calls[0]["to"] == "a@uit.ac.ma"


def test_service_unavailable_reschedules_instead_of_dropping(db, fake_tira):
    fake_tira.outcomes = [OutboundOutcome(tira_outbound.RETRY, "http 503: relay missing")]
    _make_subscription(db, "a@uit.ac.ma")
    _fan_out(db, _make_notification(db))
    db.commit()

    before = utcnow()
    summary = deliveries.process_pending_deliveries(
        db, fake_tira, max_attempts=3, batch_size=10
    )
    assert summary["retried"] == 1
    assert summary["accepted"] == 0

    db.expire_all()
    row = db.query(NotificationDeliveryModel).one()
    assert row.status == "pending"
    assert row.attempts == 1
    assert row.next_attempt_at > before
    assert "503" in row.last_error


def test_bad_request_marks_the_row_failed(db, fake_tira):
    fake_tira.outcomes = [OutboundOutcome(tira_outbound.REJECTED, "http 400: bad recipient")]
    _make_subscription(db, "a@uit.ac.ma")
    _fan_out(db, _make_notification(db))
    db.commit()

    summary = deliveries.process_pending_deliveries(
        db, fake_tira, max_attempts=3, batch_size=10
    )
    assert summary["failed"] == 1

    db.expire_all()
    row = db.query(NotificationDeliveryModel).one()
    assert row.status == "failed"
    assert "400" in row.last_error
    assert row.next_attempt_at is not None


def test_exhausted_attempts_become_dead(db, fake_tira):
    fake_tira.outcomes = [OutboundOutcome(tira_outbound.RETRY, "transport: ConnectError")]
    _make_subscription(db, "a@uit.ac.ma")
    _fan_out(db, _make_notification(db))
    db.commit()

    row = db.query(NotificationDeliveryModel).one()
    row.attempts = 2  # one more attempt reaches max_attempts=3
    db.commit()

    summary = deliveries.process_pending_deliveries(
        db, fake_tira, max_attempts=3, batch_size=10
    )
    assert summary["dead"] == 1

    db.expire_all()
    row = db.query(NotificationDeliveryModel).one()
    assert row.status == "dead"
    assert row.attempts == 3


def test_fan_out_skips_pending_and_unsubscribed(db):
    _make_subscription(db, "active@uit.ac.ma", status="active")
    _make_subscription(db, "pending@uit.ac.ma", status="pending")
    _make_subscription(db, "gone@uit.ac.ma", status="unsubscribed")

    notification = _make_notification(db)
    created = _fan_out(db, notification)
    db.commit()

    assert [row.recipient_email for row in created] == ["active@uit.ac.ma"]
    assert db.query(NotificationDeliveryModel).count() == 1


def test_fan_out_respects_institution_scope(db):
    _make_subscription(db, "all@uit.ac.ma", institution_id=None)
    _make_subscription(db, "ensias@uit.ac.ma", institution_id="inst-ensias")
    _make_subscription(db, "um5@um5.ac.ma", institution_id="inst-um5")

    notification = _make_notification(db, institution_id="inst-ensias")
    created = _fan_out(db, notification)
    db.commit()

    recipients = sorted(row.recipient_email for row in created)
    assert recipients == ["all@uit.ac.ma", "ensias@uit.ac.ma"]


def test_fan_out_is_idempotent(db):
    _make_subscription(db, "a@uit.ac.ma")
    notification = _make_notification(db)

    _fan_out(db, notification)
    db.commit()
    _fan_out(db, notification)
    db.commit()

    assert db.query(NotificationDeliveryModel).count() == 1


def test_notification_email_carries_an_unsubscribe_link_in_the_body(db):
    subscription = _make_subscription(db, "a@uit.ac.ma")
    created = _fan_out(db, _make_notification(db))
    db.commit()

    row = created[0]
    assert row.list_unsubscribe is None  # http base URL cannot fill the header
    assert "/api/v1/subscriptions/unsubscribe" in row.body
    assert row.correlation_id.startswith("ntf-")
    assert row.idempotency_key.endswith(subscription.id)


def test_https_base_url_fills_the_list_unsubscribe_header(db, monkeypatch):
    monkeypatch.setattr(settings, "JAMI3ATI_PUBLIC_BASE_URL", "https://jami3ati.ma")
    _make_subscription(db, "a@uit.ac.ma")
    created = _fan_out(db, _make_notification(db))
    db.commit()

    header = created[0].list_unsubscribe
    assert header is not None
    assert header.startswith("<https://jami3ati.ma/api/v1/subscriptions/unsubscribe?")
    assert header.endswith(">")


def test_worker_makes_no_call_when_nothing_is_due(db, fake_tira):
    _make_subscription(db, "a@uit.ac.ma")
    _fan_out(db, _make_notification(db))
    db.commit()

    deliveries.process_pending_deliveries(db, fake_tira, max_attempts=3, batch_size=10)
    summary = deliveries.process_pending_deliveries(
        db, fake_tira, max_attempts=3, batch_size=10
    )
    assert summary["processed"] == 0
    assert len(fake_tira.calls) == 1


def test_create_notification_requires_the_admin_token(client):
    payload = {
        "title_ar": "فتح الترشيح",
        "body_ar": "فُتح باب الترشيح.",
        "source_url": "https://www.ensias.ma/avis",
    }
    assert client.post("/api/v1/notifications", json=payload).status_code == 401
    assert (
        client.post(
            "/api/v1/notifications",
            json=payload,
            headers={"X-Jami3ati-Admin": "wrong"},
        ).status_code
        == 401
    )


def test_create_notification_dispatches_to_active_subscribers(client, db):
    _make_subscription(db, "active@uit.ac.ma", status="active")
    _make_subscription(db, "pending@uit.ac.ma", status="pending")

    response = client.post(
        "/api/v1/notifications",
        json={
            "institution_id": "inst-ensias",
            "title_ar": "فتح الترشيح",
            "body_ar": "فُتح باب الترشيح حتى 30 نوفمبر 2026.",
            "source_url": "https://www.ensias.ma/avis",
        },
        headers={"X-Jami3ati-Admin": settings.JAMI3ATI_ADMIN_TOKEN},
    )
    assert response.status_code == 201
    notification_id = response.json()["id"]

    rows = (
        db.query(NotificationDeliveryModel)
        .filter(NotificationDeliveryModel.notification_id == notification_id)
        .all()
    )
    assert [row.recipient_email for row in rows] == ["active@uit.ac.ma"]
    assert rows[0].status == "pending"


def test_create_notification_is_idempotent_on_dedupe_key(client, db):
    _make_subscription(db, "active@uit.ac.ma", status="active")
    payload = {
        "title_ar": "فتح الترشيح",
        "body_ar": "فُتح باب الترشيح.",
        "source_url": "https://www.ensias.ma/avis",
        "dedupe_key": "ensias:2026-11-17:avis-1",
    }
    headers = {"X-Jami3ati-Admin": settings.JAMI3ATI_ADMIN_TOKEN}

    first = client.post("/api/v1/notifications", json=payload, headers=headers)
    second = client.post("/api/v1/notifications", json=payload, headers=headers)

    assert first.status_code == 201
    assert second.status_code == 200
    assert first.json()["id"] == second.json()["id"]
    assert db.query(NotificationDeliveryModel).count() == 1


def test_notification_archive_is_public_and_ordered(client, db):
    first = _make_notification(db, title="الأول")
    second = _make_notification(db, title="الثاني")
    first.created_at = datetime(2026, 11, 16, 9, 0, 0)
    second.created_at = datetime(2026, 11, 17, 9, 0, 0)
    db.commit()

    response = client.get("/api/v1/notifications")
    assert response.status_code == 200
    titles = [item["title_ar"] for item in response.json()]
    assert titles == ["الثاني", "الأول"]
    assert all(item["source_url"].startswith("https://") for item in response.json())


def test_notification_requires_a_source_url(client):
    response = client.post(
        "/api/v1/notifications",
        json={"title_ar": "بلا مصدر", "body_ar": "نص", "source_url": "ftp://x"},
        headers={"X-Jami3ati-Admin": settings.JAMI3ATI_ADMIN_TOKEN},
    )
    assert response.status_code == 422


def test_moderator_report_notification_is_queued(client, db):
    response = client.post(
        "/api/v1/reports",
        json={
            "target_id": "inst-ensias",
            "target_name": "ENSIAS",
            "field_name": "email",
            "proposed_correction": "البريد الصحيح هو contact@ensias.ma",
            "source_proof_url": "https://www.ensias.ma/contact",
        },
    )
    assert response.status_code == 201

    row = db.query(NotificationDeliveryModel).one()
    assert row.purpose == "moderator"
    assert row.recipient_email == settings.MODERATOR_EMAIL
    assert "ENSIAS" in row.subject


def test_moderator_notification_is_skipped_when_unset(client, db, monkeypatch):
    monkeypatch.setattr(settings, "MODERATOR_EMAIL", "")
    response = client.post(
        "/api/v1/reports",
        json={
            "target_id": "inst-ensias",
            "target_name": "ENSIAS",
            "field_name": "email",
            "proposed_correction": "تصحيح",
            "source_proof_url": "https://www.ensias.ma/contact",
        },
    )
    assert response.status_code == 201
    assert db.query(NotificationDeliveryModel).count() == 0


def test_fake_client_is_reusable_across_tests(fake_tira):
    # Guards the fixture itself: a FakeTira must never reach the network.
    outcome = fake_tira.send(
        to="a@b.ma",
        subject="s",
        text="t",
        idempotency_key="k",
        correlation_id="c",
    )
    assert outcome.kind == "accepted"
    assert len(fake_tira.calls) == 1
