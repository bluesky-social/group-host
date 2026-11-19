import {
  type Agent,
  type DidString,
  type Infer,
  type LexValue,
  type RecordKeyValue,
  type RecordSchema,
  type Space,
  XrpcResponseError,
  jsonToLex,
  lexToJson,
  parseCid
} from '@atproto/lex';
import {
  RepoGapError,
  SpaceClient,
  type SpaceCollectionSchema,
  type SpaceReader
} from '@atproto/lex-space';

/** A record of a space, from one writer's repo, as JSON. */
export type GroupRecord = {
  /** `at://<group>/space/<type>/<skey>/<repo>/<collection>/<rkey>` */
  uri: string;
  /** The writer's DID. */
  repo: string;
  collection: string;
  rkey: string;
  cid: string;
  /** The record in lex's JSON form. */
  value: unknown;
};

export declare namespace GroupSpace {
  /**
   * Keeps a space's records between visits, as a JSON blob by key (e.g. in
   * IndexedDB). The key names the space and the member reading it.
   */
  type Store = {
    get(key: string): Promise<string | undefined>;
    put(key: string, value: string): Promise<void>;
  };
  type Options = SpaceClient.OpenSpaceOptions & {
    store?: Store;
    /** How many repos to pull at once. @default 4 */
    concurrency?: number;
  };
  /** A space of the group's, by the group's DID and the space's key. */
  type Ref<S extends Space> = {
    authority: DidString;
    /** The space type's literal key, or else `self`, by default. */
    skey?: RecordKeyValue<S['key']>;
  };
  type RecordsOptions = {
    /** Only these collections, by NSID or lexicon namespace. */
    collections?: readonly (string | { $type: string })[];
  };
  type Listener = (records: GroupRecord[]) => void;
  /** What a sync could not read: writers whose host failed, to be pulled again at the next sync. */
  type SyncResult = { failed: { repo: DidString; error: unknown }[] };
}

/** A writer's repo as kept: its head, and its records by path. */
type KeptRepo = {
  head: SpaceReader.RepoHead;
  records: Map<string, GroupRecord>;
};

type State = {
  /** The highest space revision listed, to list writers from. */
  cursor?: string;
  /** When the writers were last listed, in milliseconds. */
  listedAt?: number;
  repos: Map<DidString, KeptRepo>;
  /** Writers whose pull failed, to be pulled again. */
  retry: Set<DidString>;
};

/** The shape a space is kept in, in the store. */
type Stored = {
  version: typeof STORED_VERSION;
  cursor?: string;
  listedAt?: number;
  retry?: string[];
  repos: Record<
    string,
    {
      rev: string;
      state: { $bytes: string };
      records: Record<string, { cid: string; value: unknown }>;
    }
  >;
};

const STORED_VERSION = 1;

/**
 * The space host forgets a writer's removal after 72 hours: a listing from
 * a cursor older than that may miss one, so the whole set is listed instead.
 * An hour less, for clocks that disagree.
 */
const REMOVALS_KEPT_MS = 71 * 60 * 60 * 1000;

/** Errors meaning a writer's repo holds nothing readable, as lex-space's `allRecords` reads them. */
const UNAVAILABLE: Record<string, true> = {
  RepoTakendown: true,
  RepoSuspended: true,
  RepoDeactivated: true
};

/**
 * A space of a group's, read whole as the member: every writer's repo,
 * verified, kept, and read on. It wraps lex-space's `SpaceClient` (`client`),
 * which writes to the member's own repo in it.
 *
 * The first `sync()` pulls every writer's repo in full; each later one lists
 * the writers that moved since (`writers({cursor})`) and brings each up to
 * date from its kept head (`pullRepo`), dropping the writers removed from
 * the set. One sync runs at a time: a call while one runs awaits it.
 */
export class GroupSpace<S extends Space = Space> {
  readonly #store?: GroupSpace.Store;
  readonly #concurrency: number;
  readonly #listeners = new Set<GroupSpace.Listener>();
  #state?: State;
  #syncing?: Promise<GroupSpace.SyncResult>;
  #list?: GroupRecord[];

  /**
   * Opens a space of the group's for the member of `agent`. No request is
   * made until it is used.
   *
   * @throws {TypeError} if `agent` is not authenticated
   */
  static open<const S extends Space>(
    schema: S,
    { authority, skey }: GroupSpace.Ref<S>,
    { store, concurrency, ...options }: GroupSpace.Options
  ): GroupSpace<S> {
    const key = skey ?? (schema.key.startsWith('literal:') ? undefined : 'self');
    const ref = { authority, skey: key } as SpaceReader.AuthorityRef<S>;
    return new GroupSpace(SpaceClient.openSpace(schema, ref, options), {
      store,
      concurrency
    });
  }

  constructor(
    /** The lex-space client of the space, as the member. */
    readonly client: SpaceClient<S>,
    { store, concurrency = 4 }: Pick<GroupSpace.Options, 'store' | 'concurrency'> = {}
  ) {
    this.#store = store;
    this.#concurrency = concurrency;
  }

  /** The space's URI: `at://<group>/space/<type>/<skey>`. */
  get uri(): string {
    return this.client.uri;
  }

  /** The member's agent. */
  get agent(): Agent {
    return this.client.agent;
  }

  /** The key the space is kept by in the store. */
  get #key(): string {
    return `${this.client.did} ${this.client.uri}`;
  }

  /**
   * Brings the records up to date: the first time, every writer's repo in
   * full (or from the store); after, only what changed. A writer whose host
   * fails keeps its last records, is answered in `failed`, and is pulled
   * again at the next sync; the rest of the sync is kept.
   *
   * @throws {XrpcResponseError} if the writers cannot be listed, e.g.
   * `SpaceNotFound` when the member is not let in. Nothing kept changes.
   */
  sync(): Promise<GroupSpace.SyncResult> {
    this.#syncing ??= this.#sync().finally(() => {
      this.#syncing = undefined;
    });
    return this.#syncing;
  }

  /** The records held now, of every writer, or only of `collections`. */
  records({ collections }: GroupSpace.RecordsOptions = {}): GroupRecord[] {
    this.#list ??= [...(this.#state?.repos.values() ?? [])]
      .flatMap((repo) => [...repo.records.values()])
      .sort((a, b) => (a.uri < b.uri ? -1 : a.uri > b.uri ? 1 : 0));
    if (!collections) return [...this.#list];
    const wanted = new Set(collections.map((c) => (typeof c === 'string' ? c : c.$type)));
    return this.#list.filter((record) => wanted.has(record.collection));
  }

  /**
   * Calls `fn` with every record held after each sync that changed them.
   * Returns a function that stops it.
   */
  onChange(fn: GroupSpace.Listener): () => void {
    this.#listeners.add(fn);
    return () => {
      this.#listeners.delete(fn);
    };
  }

  /** Creates a record in the member's own repo in the space. */
  create<const T extends RecordSchema>(
    ns: SpaceCollectionSchema<S, T>,
    value: Omit<Infer<T>, '$type'>,
    options: { rkey?: string } = {}
  ): Promise<SpaceClient.WriteOutput> {
    return this.client.create(ns, value, options as SpaceClient.CreateOptions<T>);
  }

  /** Creates or replaces a record in the member's own repo in the space. */
  put<const T extends RecordSchema>(
    ns: SpaceCollectionSchema<S, T>,
    value: Omit<Infer<T>, '$type'>,
    { rkey }: { rkey: string }
  ): Promise<SpaceClient.WriteOutput> {
    return this.client.put(ns, value, { rkey } as SpaceClient.PutOptions<T>);
  }

  /** Deletes a record from the member's own repo in the space. */
  delete<const T extends RecordSchema>(
    ns: SpaceCollectionSchema<S, T>,
    { rkey }: { rkey: string }
  ): Promise<void> {
    return this.client.delete(ns, { rkey } as SpaceClient.DeleteOptions<T>);
  }

  async #sync(): Promise<GroupSpace.SyncResult> {
    const state = (this.#state ??= await this.#load());
    const listedAt = Date.now();
    const full =
      state.cursor === undefined ||
      state.listedAt === undefined ||
      listedAt - state.listedAt > REMOVALS_KEPT_MS;

    // A writer listed again moved while listing: its last entry is current.
    const listed = new Map<DidString, SpaceReader.ListedWriter>();
    let cursor = full ? undefined : state.cursor;
    for await (const entry of this.client.writers({ cursor })) {
      listed.set(entry.repo, entry);
      if (cursor === undefined || entry.spaceRev > cursor) cursor = entry.spaceRev;
    }

    const repos = new Map(state.repos);
    let changed = false;
    const drop = (repo: DidString) => {
      changed ||= (repos.get(repo)?.records.size ?? 0) > 0;
      repos.delete(repo);
    };

    // A full listing names every writer: one kept but not named was removed.
    if (full) for (const repo of state.repos.keys()) if (!listed.has(repo)) drop(repo);

    const moved = new Set<DidString>();
    for (const entry of listed.values()) {
      if (entry.$type === 'com.atproto.space.defs#repoRemoved') drop(entry.repo);
      else if (repos.get(entry.repo)?.head.rev !== entry.repoRev) moved.add(entry.repo);
    }
    // Writers that failed last time, unless since removed.
    for (const repo of state.retry) {
      const entry = listed.get(repo);
      if (entry?.$type === 'com.atproto.space.defs#repoRemoved') continue;
      if (full && !entry) continue;
      moved.add(repo);
    }

    const failed: GroupSpace.SyncResult['failed'] = [];
    const pulling = [...moved];
    for (let i = 0; i < pulling.length; i += this.#concurrency) {
      const batch = pulling.slice(i, i + this.#concurrency);
      const pulls = await Promise.allSettled(
        batch.map((repo) => this.#pull(repo, repos.get(repo)))
      );
      for (const [j, settled] of pulls.entries()) {
        const repo = batch[j]!;
        if (settled.status === 'rejected') {
          failed.push({ repo, error: settled.reason });
          continue;
        }
        const pull = settled.value;
        if (pull === 'drop') drop(repo);
        else if (pull) {
          changed ||= differs(repos.get(repo)?.records, pull.records);
          repos.set(repo, pull);
        }
      }
    }

    const retry = new Set(failed.map((f) => f.repo));
    const retryMoved =
      retry.size !== state.retry.size || [...retry].some((repo) => !state.retry.has(repo));
    const next: State = { cursor, listedAt, repos, retry };
    if (this.#store && (changed || retryMoved || cursor !== state.cursor)) {
      await this.#store.put(this.#key, JSON.stringify(stored(next)));
    }
    this.#state = next;
    if (changed) {
      this.#list = undefined;
      const records = this.records();
      for (const fn of this.#listeners) fn(records);
    }
    return { failed };
  }

  /**
   * Brings one writer's repo up to date from what is kept of it. Answers the
   * repo as it is now, `drop` when it holds nothing readable, or nothing when
   * what is kept is to stay (its host has an older copy than the one kept).
   */
  async #pull(repo: DidString, kept: KeptRepo | undefined): Promise<KeptRepo | 'drop' | undefined> {
    let pull: SpaceReader.RepoPull;
    try {
      pull = await this.client.pullRepo(
        kept
          ? {
              repo,
              head: kept.head,
              lookup: (collection, rkey) => {
                const record = kept.records.get(`${collection}/${rkey}`);
                return record && parseCid(record.cid);
              }
            }
          : { repo }
      );
    } catch (err) {
      if (err instanceof RepoGapError) return undefined;
      if (err instanceof XrpcResponseError && UNAVAILABLE[err.error] === true) return 'drop';
      throw err;
    }

    switch (pull.type) {
      case 'empty':
        return 'drop';
      case 'snapshot': {
        const records = new Map<string, GroupRecord>();
        for (const { collection, rkey, cid, record } of pull.records) {
          records.set(
            `${collection}/${rkey}`,
            this.#record(repo, collection, rkey, cid.toString(), lexToJson(record))
          );
        }
        return { head: pull.head, records };
      }
      case 'changes': {
        const records = new Map(kept?.records);
        for (const { collection, rkey, cid, record } of pull.changes) {
          const path = `${collection}/${rkey}`;
          if (cid && record) {
            records.set(
              path,
              this.#record(repo, collection, rkey, cid.toString(), lexToJson(record))
            );
          } else records.delete(path);
        }
        return { head: pull.head, records };
      }
    }
  }

  #record(
    repo: string,
    collection: string,
    rkey: string,
    cid: string,
    value: unknown
  ): GroupRecord {
    return {
      uri: `${this.client.uri}/${repo}/${collection}/${rkey}`,
      repo,
      collection,
      rkey,
      cid,
      value
    };
  }

  /** What the store keeps of the space, or nothing kept. */
  async #load(): Promise<State> {
    const json = await this.#store?.get(this.#key);
    const kept = json === undefined ? undefined : parseStored(json);
    if (!kept) return { repos: new Map(), retry: new Set() };
    const repos = new Map<DidString, KeptRepo>();
    for (const [repo, { rev, state, records }] of Object.entries(kept.repos)) {
      const held = new Map<string, GroupRecord>();
      for (const [path, { cid, value }] of Object.entries(records)) {
        const slash = path.indexOf('/');
        const collection = path.slice(0, slash);
        const rkey = path.slice(slash + 1);
        held.set(path, this.#record(repo, collection, rkey, cid, value));
      }
      repos.set(repo as DidString, {
        head: { rev, state: jsonToLex(state) as Uint8Array },
        records: held
      });
    }
    return {
      cursor: kept.cursor,
      listedAt: kept.listedAt,
      repos,
      retry: new Set((kept.retry ?? []) as DidString[])
    };
  }
}

/** Whether two copies of a repo's records differ. */
function differs(
  before: Map<string, GroupRecord> | undefined,
  after: Map<string, GroupRecord>
): boolean {
  if ((before?.size ?? 0) !== after.size) return true;
  for (const [path, record] of after) if (before?.get(path)?.cid !== record.cid) return true;
  return false;
}

function stored({ cursor, listedAt, repos, retry }: State): Stored {
  const out: Stored = {
    version: STORED_VERSION,
    cursor,
    listedAt,
    ...(retry.size ? { retry: [...retry] } : {}),
    repos: {}
  };
  for (const [repo, { head, records }] of repos) {
    const kept: Stored['repos'][string]['records'] = {};
    for (const [path, { cid, value }] of records) kept[path] = { cid, value };
    out.repos[repo] = {
      rev: head.rev,
      state: lexToJson(head.state as LexValue) as { $bytes: string },
      records: kept
    };
  }
  return out;
}

/**
 * The space as the store keeps it, or nothing when what is kept is not of
 * this version, or not JSON: a copy to be read again in full.
 */
function parseStored(json: string): Stored | undefined {
  try {
    const kept = JSON.parse(json) as Stored;
    return kept?.version === STORED_VERSION ? kept : undefined;
  } catch {
    return undefined;
  }
}
