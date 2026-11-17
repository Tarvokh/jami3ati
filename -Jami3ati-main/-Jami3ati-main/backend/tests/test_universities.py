from fastapi.testclient import TestClient
from app.main import app

client = TestClient(app)


def test_get_universities():
    response = client.get("/api/v1/universities")
    assert response.status_code == 200
    assert isinstance(response.json(), list)


def test_university_not_found():
    response = client.get("/api/v1/universities/NON_EXISTENT_UNIV")
    assert response.status_code == 404
