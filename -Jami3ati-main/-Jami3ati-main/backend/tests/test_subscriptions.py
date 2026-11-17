from datetime import timedelta
from urllib.parse import parse_qs, urlparse

from app.core.clock import utcnow
from app.core.config import settings
from app.models.notification import NotificationDeliveryModel
from app.models.subscription import SubscriptionModel


def _queued_confirmation(db, subscription_id):
    return (
        db.query(NotificationDeliveryModel)
        .filter(NotificationDeliveryModel.subscription_id == subscription_id)
        .filter(NotificationDeliveryModel.purpose == "confirm")
        .order_by(NotificationDeliveryModel.created_at.desc())
        .first()
    )


def _link_from_body(body: str) -> str:
    for line in body.splitlines():
        if "/api/v1/subscriptions/confirm" in line:
            return line.strip()
    raise AssertionError("confirmation link missing from the queued body")


def _query(url: str) -> dict:
    return {key: values[0] for key, values in parse_qs(urlparse(url).query).items()}


def _signup(client, email="student@uit.ac.ma", institution_id=None):
    payload = {"email": email}
    if institution_id:
        payload["institution_id"] = institution_id
    return client.post("/api/v1/subscriptions", json=payload)


def test_signup_is_pending_and_queues_one_confirmation(client, db):
    response = _signup(client, institution_id="inst-ensias")
    assert response.status_code == 201
    body = response.json()
    assert body["status"] == "pending"
    assert body["email"] == "student@uit.ac.ma"

    row = db.query(SubscriptionModel).filter(SubscriptionModel.id == body["id"]).one()
    assert row.status == "pending"
    assert row.confirm_token_hash is not None
    assert row.confirm_expires_at > utcnow()

    delivery = _queued_confirmation(db, row.id)
    assert delivery is not None
    assert delivery.status == "pending"
    assert delivery.recipient_email == "student@uit.ac.ma"
    assert "تأكيد الاشتراك" in delivery.subject
    assert _link_from_body(delivery.body).startswith("http://testserver/")
    # Transactional confirmation: no list header on a message that is not yet
    # part of a list.
    assert delivery.list_unsubscribe is None


def test_duplicate_signup_rotates_the_token(client, db):
    first = _signup(client).json()
    db.expire_all()
    first_delivery = _queued_confirmation(db, first["id"])
    old_link = _link_from_body(first_delivery.body)
    # Push the first delivery into the past so "latest" is unambiguous even
    # when both rows land in the same microsecond.
    first_delivery.created_at = utcnow() - timedelta(hours=1)
    db.commit()

    second = _signup(client).json()
    assert second["id"] == first["id"]  # same row, not a duplicate address
    db.expire_all()
    new_link = _link_from_body(_queued_confirmation(db, first["id"]).body)
    assert new_link != old_link

    stale = _query(old_link)
    rejected = client.get(
        "/api/v1/subscriptions/confirm",
        params={"id": stale["id"], "token": stale["token"]},
    )
    assert rejected.status_code == 400

    fresh = _query(new_link)
    accepted = client.get(
        "/api/v1/subscriptions/confirm",
        params={"id": fresh["id"], "token": fresh["token"]},
    )
    assert accepted.status_code == 200
    assert accepted.json()["status"] == "active"


def test_confirm_activates_the_subscription(client, db):
    created = _signup(client).json()
    db.expire_all()
    link = _query(_link_from_body(_queued_confirmation(db, created["id"]).body))

    response = client.get("/api/v1/subscriptions/confirm", params=link)
    assert response.status_code == 200
    assert response.json() == {"id": created["id"], "status": "active"}

    db.expire_all()
    row = db.query(SubscriptionModel).filter(SubscriptionModel.id == created["id"]).one()
    assert row.status == "active"
    assert row.confirmed_at is not None
    assert row.confirm_token_hash is None
    assert row.confirm_expires_at is None


def test_confirm_is_idempotent(client, db):
    created = _signup(client).json()
    db.expire_all()
    link = _query(_link_from_body(_queued_confirmation(db, created["id"]).body))

    assert client.get("/api/v1/subscriptions/confirm", params=link).status_code == 200
    again = client.get("/api/v1/subscriptions/confirm", params=link)
    assert again.status_code == 200
    assert again.json()["status"] == "active"


def test_confirm_rejects_a_tampered_token(client, db):
    created = _signup(client).json()
    db.expire_all()
    link = _query(_link_from_body(_queued_confirmation(db, created["id"]).body))
    link["token"] = link["token"] + "tampered"

    response = client.get("/api/v1/subscriptions/confirm", params=link)
    assert response.status_code == 400
    db.expire_all()
    row = db.query(SubscriptionModel).filter(SubscriptionModel.id == created["id"]).one()
    assert row.status == "pending"


def test_confirm_rejects_an_expired_token(client, db):
    created = _signup(client).json()
    db.expire_all()
    link = _query(_link_from_body(_queued_confirmation(db, created["id"]).body))

    row = db.query(SubscriptionModel).filter(SubscriptionModel.id == created["id"]).one()
    row.confirm_expires_at = utcnow() - timedelta(minutes=1)
    db.commit()

    response = client.get("/api/v1/subscriptions/confirm", params=link)
    assert response.status_code == 400
    db.expire_all()
    row = db.query(SubscriptionModel).filter(SubscriptionModel.id == created["id"]).one()
    assert row.status == "pending"


def test_confirm_rejects_an_unknown_id(client):
    response = client.get(
        "/api/v1/subscriptions/confirm",
        params={"id": "sub-doesnotexist", "token": "whatever"},
    )
    assert response.status_code == 400


def test_unsubscribe_get(client, db):
    created = _signup(client).json()
    db.expire_all()
    confirm = _query(_link_from_body(_queued_confirmation(db, created["id"]).body))
    client.get("/api/v1/subscriptions/confirm", params=confirm)

    from app.core import tokens

    unsubscribe = _query(
        tokens.unsubscribe_link(
            settings.JAMI3ATI_PUBLIC_BASE_URL,
            settings.JAMI3ATI_HMAC_SECRET,
            created["id"],
        )
    )
    response = client.get("/api/v1/subscriptions/unsubscribe", params=unsubscribe)
    assert response.status_code == 200
    assert response.json()["status"] == "unsubscribed"

    db.expire_all()
    row = db.query(SubscriptionModel).filter(SubscriptionModel.id == created["id"]).one()
    assert row.status == "unsubscribed"
    assert row.unsubscribed_at is not None


def test_unsubscribe_one_click_post(client, db):
    created = _signup(client).json()
    db.expire_all()
    confirm = _query(_link_from_body(_queued_confirmation(db, created["id"]).body))
    client.get("/api/v1/subscriptions/confirm", params=confirm)

    from app.core import tokens

    unsubscribe = _query(
        tokens.unsubscribe_link(
            settings.JAMI3ATI_PUBLIC_BASE_URL,
            settings.JAMI3ATI_HMAC_SECRET,
            created["id"],
        )
    )
    # RFC 8058: the mail client POSTs to the List-Unsubscribe URL.
    response = client.post("/api/v1/subscriptions/unsubscribe", params=unsubscribe)
    assert response.status_code == 200
    assert response.json()["status"] == "unsubscribed"


def test_unsubscribe_rejects_a_wrong_token(client, db):
    created = _signup(client).json()
    response = client.get(
        "/api/v1/subscriptions/unsubscribe",
        params={"id": created["id"], "token": "not-the-mac"},
    )
    assert response.status_code == 400
    db.expire_all()
    row = db.query(SubscriptionModel).filter(SubscriptionModel.id == created["id"]).one()
    assert row.status == "pending"


def test_signup_fails_closed_without_a_secret(client, monkeypatch):
    monkeypatch.setattr(settings, "JAMI3ATI_HMAC_SECRET", "")
    response = _signup(client)
    assert response.status_code == 503


def test_signup_rejects_an_invalid_email(client):
    response = client.post("/api/v1/subscriptions", json={"email": "not-an-email"})
    assert response.status_code == 422
