/**
 * Where the group host serves an image of `did`'s, by the CID a record
 * names it by, for whoever may see that record: none for no image.
 */
export const image = (did: string, cid?: string | null) => (cid ? `/img/${did}/${cid}` : null);

/**
 * Where the group host serves an app's icon, read by the host from the
 * `logo_uri` its client metadata names, so the browser never fetches it
 * from the app: none where the host gives no `logo`.
 */
export const appLogo = (client?: { clientId: string; logo?: string }) =>
  client?.logo ? `/img/app?id=${encodeURIComponent(client.clientId)}` : null;
