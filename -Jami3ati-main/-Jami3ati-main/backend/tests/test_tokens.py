from app.core import tokens


def test_confirm_token_is_random_and_long_enough():
    first = tokens.new_confirm_token()
    second = tokens.new_confirm_token()
    assert first != second
    assert len(first) >= 32


def test_confirm_digest_hides_the_token():
    raw = tokens.new_confirm_token()
    digest = tokens.confirm_token_digest(raw)
    assert digest != raw
    assert len(digest) == 64
    # Stable: the same token must produce the same digest at verification time.
    assert tokens.confirm_token_digest(raw) == digest


def test_unsubscribe_token_is_deterministic():
    secret = "s3cret"
    first = tokens.unsubscribe_token(secret, "sub-abc")
    second = tokens.unsubscribe_token(secret, "sub-abc")
    assert first == second
    assert first != tokens.unsubscribe_token(secret, "sub-xyz")
    assert first != tokens.unsubscribe_token("other-secret", "sub-abc")


def test_unsubscribe_token_rejects_tampering():
    secret = "s3cret"
    token = tokens.unsubscribe_token(secret, "sub-abc")
    assert tokens.unsubscribe_token_matches(secret, "sub-abc", token) is True
    assert tokens.unsubscribe_token_matches(secret, "sub-abc", token + "x") is False
    assert tokens.unsubscribe_token_matches(secret, "sub-abc", "") is False
    assert tokens.unsubscribe_token_matches("other-secret", "sub-abc", token) is False


def test_links_are_absolute_and_well_formed():
    secret = "s3cret"
    confirm = tokens.confirmation_link("http://testserver/", "sub-abc", "raw")
    assert confirm == "http://testserver/api/v1/subscriptions/confirm?id=sub-abc&token=raw"

    unsubscribe = tokens.unsubscribe_link("https://example.ma/", secret, "sub-abc")
    assert unsubscribe.startswith("https://example.ma/api/v1/subscriptions/unsubscribe?id=sub-abc&token=")
    presented = unsubscribe.split("token=", 1)[1]
    assert tokens.unsubscribe_token_matches(secret, "sub-abc", presented)
