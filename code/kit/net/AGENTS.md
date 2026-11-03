# Requests only where they may go: notes for coding agents

`code-villain-net` keeps a server's requests to addresses someone else gave
it (a picture's, a newsletter's way out, a runner's, a health check's) away
from this machine, private networks and the cloud's metadata service.
Almanac, Raven, Forge and Postrider use it.

Checking the name in the URL isn't enough: a public name can resolve to
`127.0.0.1` or `169.254.169.254`, and a public page can redirect there. So
the kit checks the addresses a name resolves to, as the connection is made,
and every redirect again.

## Where requests may go

- `Reach::Public`: the internet only. Never loopback, private networks
  (10/8, 172.16/12, 192.168/16, `fc00::/7`), carrier NAT and tailnets
  (100.64/10), links (169.254/16, `fe80::/10`), multicast, broadcast,
  unspecified, documentation, benchmark or reserved ranges. IPv4 in IPv6
  (mapped, compatible, NAT64, 6to4) is judged as itself. Names like
  `localhost`, `*.local`, `*.internal` and `*.home.arpa` are refused before
  they're looked up.
- `Reach::OwnNetworks`: everything but link-local addresses and cloud
  metadata (169.254/16, `fe80::/10`, 100.100.100.200, `fd00:ec2::254`,
  `metadata.google.internal`). For an address an admin gives on purpose,
  such as an app on this Mac or the company's network: Raven's health
  checks, Forge's runners.

## Using it

- `code_villain_net::client(reach)` is a `reqwest::ClientBuilder` with the
  kit's `Resolver`, a redirect policy that checks each one (at most
  `REDIRECTS`), and no proxy, since a proxy would resolve the name itself.
  Add a timeout and a user agent; `.redirect(Policy::none())` for none,
  `.https_only(true)` for https only.
- Check the first URL with `reach.allows_url(&url)` (or `public(&url)`)
  before sending: an address in the URL never reaches the resolver.
- `public_ip` and `link_or_metadata` are the address rules alone.

## Changing it

A change here changes every app that uses it: run `cargo test` here, then
each app's tests. An app that depends on it copies `kit/net` in its
`Dockerfile` and lets it through its `Dockerfile.dockerignore`.
