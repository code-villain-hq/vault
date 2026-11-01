//! Company sign-in for any studio app server: OpenID Connect, sessions, and
//! the desktop handoff. See `AGENTS.md`.

pub mod email;
pub mod oidc;
pub mod session;
pub mod this_mac;

pub use email::{AGENT_EMAIL_DOMAIN, is_agent_email, normalize_email};
pub use this_mac::{EMAIL as THIS_MAC_EMAIL, ensure_person as ensure_this_mac_person, person as this_mac_person};
pub use oidc::{
    CallbackParams, DESKTOP_RETURN, DesktopRequest, LinkPolicy, OidcHost, OidcLogin, Problem,
    StartParams, callback, desktop, lock_membership, safe_redirect, start,
};
pub use session::{
    SESSION_DAYS, SessionNames, clear_session_cookie, end_session, hash_token, member_role,
    new_token, session_cookie, session_token, start_session, still_member, user_for_session,
};
