# Tira Mail - Rspamd milter endpoint for Stalwart's x:MtaMilter.
#
# Only the proxy worker speaks Milter. It is bound to loopback on purpose:
# the Milter protocol has no authentication or encryption, so exposing 11332
# to a network is equivalent to exposing an open relay scanner.
#
# Self-scan mode (no separate scan layer) is correct for a single node. When
# volume grows, replace `self_scan` with a `hosts = "round-robin:..."` scan
# layer; the Stalwart side does not change.
bind_socket = "127.0.0.1:11332";
milter = yes;
timeout = 120s;

# Stable header name for the Tira UI and support tooling. Never reuse
# X-Spam-Status from SpamAssassin: two scanners writing one header makes
# tickets unreadable.
spam_header = "X-Spam";

upstream "local" {
  default = yes;
  self_scan = yes;
}
