"""The moderation surface must not be reachable without the admin token.

`PATCH /reports/{id}/status` used to be unauthenticated: any caller could
approve an unverified accusation against an institution. These tests pin the
guard on the real app (no mocks of the guard itself) and prove that the public
submission path still works.
"""

from app.core.config import settings

ADMIN_HEADER = "X-Jami3ati-Admin"


def _submit(client, **overrides):
    payload = {
        "target_id": "inst-ensias",
        "target_name": "ENSIAS",
        "field_name": "frais",
        "current_value": "5000 MAD",
        "proposed_correction": "4000 MAD",
        "source_proof_url": "https://www.ensias.ma/avis-2026",
        "user_notes": None,
        "reporter_email": "citizen@example.ma",
    }
    payload.update(overrides)
    return client.post("/api/v1/reports", json=payload)


def test_submission_stays_public_and_pending(client):
    response = _submit(client)
    assert response.status_code == 201
    body = response.json()
    assert body["status"] == "pending"
    assert body["id"].startswith("rep-")


def test_patch_without_token_is_rejected_and_changes_nothing(client):
    report_id = _submit(client).json()["id"]

    response = client.patch(
        f"/api/v1/reports/{report_id}/status", json={"status": "approved"}
    )
    assert response.status_code == 401

    # Read back through the public list: the row must still be pending.
    listed = {row["id"]: row for row in client.get("/api/v1/reports").json()}
    assert listed[report_id]["status"] == "pending"


def test_patch_with_a_wrong_token_is_rejected(client):
    report_id = _submit(client).json()["id"]

    response = client.patch(
        f"/api/v1/reports/{report_id}/status",
        json={"status": "approved"},
        headers={ADMIN_HEADER: "not-the-token"},
    )
    assert response.status_code == 401


def test_patch_with_the_admin_token_is_accepted(client):
    report_id = _submit(client).json()["id"]

    response = client.patch(
        f"/api/v1/reports/{report_id}/status",
        json={"status": "approved"},
        headers={ADMIN_HEADER: settings.JAMI3ATI_ADMIN_TOKEN},
    )
    assert response.status_code == 200
    assert response.json()["status"] == "approved"


def test_unset_admin_token_fails_closed(client, monkeypatch):
    report_id = _submit(client).json()["id"]
    monkeypatch.setattr(settings, "JAMI3ATI_ADMIN_TOKEN", "")

    response = client.patch(
        f"/api/v1/reports/{report_id}/status",
        json={"status": "approved"},
        headers={ADMIN_HEADER: ""},
    )
    assert response.status_code == 503


def test_unknown_report_still_needs_the_token_first(client):
    response = client.patch(
        "/api/v1/reports/rep-doesnotexist/status", json={"status": "approved"}
    )
    # 401 before 404: an anonymous caller must not be able to probe which
    # report ids exist.
    assert response.status_code == 401
