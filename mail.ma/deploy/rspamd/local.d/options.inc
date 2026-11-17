# Tira Mail - global Rspamd options.
#
# Stalwart and Rspamd share a host, so the only "local" network is loopback.
# Do not widen this to RFC1918 ranges on a shared network: every host in those
# ranges would be treated as trusted and skip SPF.
local_addrs = "127.0.0.0/8, ::1";

# DNS is deliberately left to the system resolver. Hardcoding 8.8.8.8 on a
# sovereign deployment leaks every DNSBL/SPF lookup to a third country and
# breaks when egress DNS is filtered. If the node has a local filtering
# resolver, uncomment and point at it:
#
# dns {
#   nameserver = ["127.0.0.1:53"];
#   timeout = 2s;
#   retransmits = 2;
# }

# A scan that exceeds the task timeout must not silently become "ham".
# Stalwart returns 4xx when the milter fails (tempFailOnError = true), so the
# sender retries later instead of the message bypassing the filter.
soft_reject_on_timeout = true;
task_timeout = 15s;
