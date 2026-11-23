// Why the host refused, in the console's words. The host's own `message`
// is English for whoever reads its logs. The console tells one refusal
// from another by its `error` name, as a method's lexicon lists them, and
// says each in the reader's language.

import type { LexErrorData } from '@atproto/lex';
import { m } from '#lib/paraglide/messages.js';

/** A refusal as the host sends it, or nothing where its body wasn't JSON. */
export type Failure = Partial<LexErrorData> | null | undefined;

/** What each name the host answers is said as, wherever the console meets it. */
const SAID: Record<string, () => string> = {
  AccountCreationFailed: m.error_account_creation_failed,
  ActorNotFound: m.error_actor_not_found,
  AlreadyAdded: m.error_already_added,
  AlreadyDeactivated: m.error_already_deactivated,
  AlreadyHere: m.error_already_here,
  AlreadyMember: m.error_already_member,
  AuthMissing: m.error_auth_missing,
  AuthRequired: m.error_auth_missing,
  AuthenticationRequired: m.error_auth_missing,
  BadJwt: m.error_bad_jwt,
  BadJwtAudience: m.error_bad_jwt,
  BadJwtIss: m.error_bad_jwt,
  BadJwtLexiconMethod: m.error_bad_jwt,
  BlobNotFound: m.error_blob_not_found,
  BlobTooLarge: m.error_blob_too_large,
  DuplicateName: m.error_duplicate_name,
  ExpiredRequest: m.error_expired_request,
  ExpiredToken: m.error_expired_token,
  Forbidden: m.error_forbidden,
  GroupNotFound: m.error_group_not_found,
  HandleNotAvailable: m.error_handle_not_available,
  HandleTaken: m.error_handle_taken,
  HostKey: m.error_host_key,
  InternalServerError: m.error_internal,
  InvalidArchive: m.error_invalid_archive,
  InvalidHandle: m.error_invalid_handle,
  InvalidInvite: m.error_invalid_invite,
  InvalidKey: m.error_invalid_key,
  InvalidName: m.error_invalid_name,
  InvalidOperation: m.error_invalid_operation,
  InvalidRepo: m.error_invalid_repo,
  InvalidRequest: m.error_invalid_request,
  InvalidScope: m.error_invalid_scope,
  InvalidSite: m.error_invalid_site,
  InvalidSubject: m.error_invalid_subject,
  InvalidSwap: m.error_invalid_swap,
  InvalidToken: m.error_invalid_token,
  InviteNotFound: m.error_invite_not_found,
  JoinClosed: m.error_join_closed,
  JoinRequestNotFound: m.error_join_request_not_found,
  LastConfigurer: m.error_last_configurer,
  NoOwnDomain: m.error_no_own_domain,
  NotAMember: m.error_not_a_member,
  NotAllowed: m.error_not_allowed,
  NotAuthorized: m.error_not_authorized,
  NotController: m.error_not_controller,
  NotDeactivated: m.error_not_deactivated,
  NotFound: m.error_not_found,
  NotPlc: m.error_not_plc,
  NotPointing: m.error_not_pointing,
  NotReaching: m.error_not_reaching,
  NotSpaceOwner: m.error_not_space_owner,
  NotStarted: m.error_not_started,
  PayloadTooLarge: m.error_payload_too_large,
  RateLimitExceeded: m.error_rate_limit_exceeded,
  RecordNotFound: m.error_record_not_found,
  RepoDeactivated: m.error_repo_deactivated,
  RepoNotFound: m.error_repo_not_found,
  ReservedSpace: m.error_reserved_space,
  SameAccount: m.error_same_account,
  ScopeMissing: m.error_scope_missing,
  SignInAgain: m.error_sign_in_again,
  SpaceAlreadyExists: m.error_space_already_exists,
  SpaceNotFound: m.error_space_not_found,
  SpaceReserved: m.error_reserved_space,
  SubjectNotFound: m.error_subject_not_found,
  TooLarge: m.error_too_large,
  TooManyAppPasswords: m.error_too_many_app_passwords,
  TooManyKeys: m.error_too_many_keys,
  UnknownKey: m.error_unknown_key,
  UnknownRole: m.error_unknown_role,
  UnsupportedDomain: m.error_unsupported_domain,
  UnsupportedPolicy: m.error_unsupported_policy,
  UntrustedApp: m.error_untrusted_app,
  UpstreamFailure: m.error_upstream_failure
};

/**
 * The names the host gives for more than one reason, its `message` saying
 * which: what a lexicon or a handler found wrong with the input
 * (`InvalidRequest`, "label is lowercase letters and hyphens"), a handle,
 * a site or an archive, or what another server said. The reader needs that
 * reason to fix what they sent, and the console can't know it, so for
 * these the host's own words are said. They are English: a language other
 * than English needs the host to name each reason first.
 */
const REASONED: Record<string, true> = {
  InvalidRequest: true,
  InvalidHandle: true,
  InvalidSite: true,
  InvalidArchive: true,
  UpstreamFailure: true
};

/**
 * Why the host refused, with `status`, as the reader is told: `own`'s
 * words for a name where a page has better ones (a domain it names), else
 * the host's reason for a name it gives several for, else the console's
 * for that name, else that it failed.
 */
export function refusal(
  failure: Failure,
  status: number,
  own: Record<string, () => string> = {}
): string {
  const name = failure?.error ?? '';
  if (own[name]) return own[name]();
  if (REASONED[name] && failure?.message) return failure.message;
  const said = SAID[name];
  if (said) return said();
  return status >= 500 ? m.error_host_failed() : m.error_unknown();
}

/** Why the host refused `res`, read from its body, as `refusal` says it. */
export async function refused(res: Response, own?: Record<string, () => string>): Promise<string> {
  const failure: Failure = await res.json().catch(() => null);
  return refusal(failure, res.status, own);
}
