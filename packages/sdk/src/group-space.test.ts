import {
  type Cid,
  type DidString,
  type LexMap,
  XrpcResponseError,
  cidForLex,
  l
} from '@atproto/lex';
import { RepoGapError, type SpaceReader, lexicons } from '@atproto/lex-space';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { type GroupRecord, GroupSpace } from './group-space.ts';

const GROUP = 'did:plc:group';
const ALICE = 'did:plc:alice';
const BOB = 'did:plc:bob';
const CAROL = 'did:plc:carol';

const post = l.record('tid', 'com.example.post', l.object({ text: l.string() }));
const reaction = l.record('tid', 'com.example.reaction', l.object({ emoji: l.string() }));
const room = l.space('any', 'com.example.room', [post.$type, reaction.$type]);
const SPACE = `at://${GROUP}/space/${room.nsid}/self`;

type Kept = { collection: string; rkey: string; cid: Cid; record: LexMap };
type Version = { rev: string; records: Map<string, Kept> };
type Write = { collection: string; rkey: string; value?: LexMap };

const tid = (n: number) => `3m${String(n).padStart(11, '0')}`;
const headOf = (version: Version): SpaceReader.RepoHead => ({
  rev: version.rev,
  state: new TextEncoder().encode(version.rev)
});
const commitOf = (version: Version) =>
  ({ rev: version.rev }) as unknown as SpaceReader.RepoChanges['commit'];

const xrpcError = (error: string) =>
  new XrpcResponseError(
    lexicons.com.atproto.space.getRepo.main,
    new Response(null, { status: 400 }),
    { encoding: 'application/json', body: { error, message: error } }
  );

/**
 * A space as its hosts hold it: each writer's repo, version by version, and
 * the writer set as `listWriters` answers it. It answers in place of
 * lex-space's `writers`, `syncRepo` and `getRepoSnapshot`, so `pullRepo`
 * runs as it is.
 */
class World {
  #spaceRev = 0;
  #rev = 0;
  readonly listed = new Map<string, SpaceReader.ListedWriter>();
  readonly repos = new Map<string, Version[]>();
  /** Repos whose operation log no longer reaches back to any kept head. */
  readonly gaps = new Set<string>();
  /** Repos whose host answers this error. */
  readonly failing = new Map<string, Error>();

  async write(repo: string, writes: Write[]) {
    const versions = this.repos.get(repo) ?? [];
    const records = new Map(versions.at(-1)?.records);
    for (const { collection, rkey, value } of writes) {
      const path = `${collection}/${rkey}`;
      if (value) {
        const record = { $type: collection, ...value };
        records.set(path, {
          collection,
          rkey,
          cid: await cidForLex(record),
          record
        });
      } else records.delete(path);
    }
    const rev = tid(++this.#rev);
    versions.push({ rev, records });
    this.repos.set(repo, versions);
    this.#list({
      $type: 'com.atproto.space.defs#repoWrite',
      repo: repo as DidString,
      repoRev: rev as l.TidString,
      data: new Uint8Array(),
      spaceRev: tid(++this.#spaceRev) as l.TidString
    });
  }

  remove(repo: string) {
    this.#list({
      $type: 'com.atproto.space.defs#repoRemoved',
      repo: repo as DidString,
      spaceRev: tid(++this.#spaceRev) as l.TidString
    });
  }

  /** The host forgets a removal, as it does after 72 hours. */
  forget(repo: string) {
    this.listed.delete(repo);
  }

  #list(entry: SpaceReader.ListedWriter) {
    this.listed.delete(entry.repo);
    this.listed.set(entry.repo, entry);
  }

  async *writers({ cursor }: SpaceReader.ListWritersOptions = {}) {
    for (const entry of this.listed.values()) {
      if (cursor === undefined || entry.spaceRev > cursor) yield entry;
    }
  }

  #latest(repo: string): Version {
    const failure = this.failing.get(repo);
    if (failure) throw failure;
    const version = this.repos.get(repo)?.at(-1);
    if (!version) throw xrpcError('RepoNotFound');
    return version;
  }

  async syncRepo({ repo, head, lookup }: SpaceReader.SyncRepoOptions) {
    const to = this.#latest(repo);
    const from = this.repos.get(repo)!.find((v) => v.rev === head.rev);
    if (!from || this.gaps.has(repo)) throw new RepoGapError(`Repo ${repo} has a gap`);
    const changes: SpaceReader.RecordChange[] = [];
    for (const path of new Set([...from.records.keys(), ...to.records.keys()])) {
      const before = from.records.get(path);
      const after = to.records.get(path);
      if (before?.cid.toString() === after?.cid.toString()) continue;
      const { collection, rkey } = (before ?? after)!;
      const kept = await lookup(collection as l.NsidString, rkey);
      if (kept?.toString() !== before?.cid.toString()) {
        throw new RepoGapError(`Repo ${repo} diverged at ${path}`);
      }
      changes.push({
        collection: collection as l.NsidString,
        rkey,
        cid: after?.cid ?? null,
        prev: before?.cid ?? null,
        record: after?.record
      });
    }
    return { commit: commitOf(to), head: headOf(to), changes };
  }

  async getRepoSnapshot({ repo }: { repo: DidString }) {
    const version = this.#latest(repo);
    return {
      commit: commitOf(version),
      head: headOf(version),
      records: [...version.records.values()].map(({ collection, rkey, cid, record }) => ({
        collection: collection as l.NsidString,
        rkey,
        cid,
        record
      }))
    };
  }
}

const memoryStore = () => {
  const kept = new Map<string, string>();
  return {
    kept,
    get: async (key: string) => kept.get(key),
    put: async (key: string, value: string) => {
      kept.set(key, value);
    }
  };
};

/** The space as Alice reads it, with its network answered by `world`. */
function openRoom(world: World, options: Partial<GroupSpace.Options> = {}) {
  const space = GroupSpace.open(
    room,
    { authority: GROUP },
    {
      agent: {
        did: ALICE,
        fetchHandler: () => Promise.reject(new Error('no network in tests'))
      },
      ...options
    }
  );
  const calls = {
    writers: vi.spyOn(space.client, 'writers').mockImplementation((o) => world.writers(o)),
    syncRepo: vi.spyOn(space.client, 'syncRepo').mockImplementation((o) => world.syncRepo(o)),
    getRepoSnapshot: vi
      .spyOn(space.client, 'getRepoSnapshot')
      .mockImplementation((o) => world.getRepoSnapshot(o))
  };
  return { space, calls };
}

const repos = (calls: { mock: { calls: [{ repo: string }][] } }) =>
  calls.mock.calls.map(([options]) => options.repo);
const cursors = (calls: { mock: { calls: [SpaceReader.ListWritersOptions?][] } }) =>
  calls.mock.calls.map(([options]) => options?.cursor);
const values = (records: GroupRecord[]) =>
  records.map(({ repo, collection, rkey, value }) => [repo, collection, rkey, value]);

const text = (t: string) => ({ text: t });

async function world() {
  const w = new World();
  await w.write(ALICE, [{ collection: post.$type, rkey: 'a1', value: text('hello') }]);
  await w.write(BOB, [
    { collection: post.$type, rkey: 'b1', value: text('hi') },
    { collection: reaction.$type, rkey: 'b2', value: { emoji: '👋' } }
  ]);
  return w;
}

afterEach(() => {
  vi.useRealTimers();
});

describe(GroupSpace, () => {
  it('reads every writer in full first, then reads on from the heads it keeps', async () => {
    const w = await world();
    const { space, calls } = openRoom(w);

    await space.sync();
    expect(repos(calls.getRepoSnapshot)).toEqual([ALICE, BOB]);
    expect(space.records()).toEqual([
      {
        uri: `${SPACE}/${ALICE}/${post.$type}/a1`,
        repo: ALICE,
        collection: post.$type,
        rkey: 'a1',
        cid: (await cidForLex({ $type: post.$type, text: 'hello' })).toString(),
        value: { $type: post.$type, text: 'hello' }
      },
      expect.objectContaining({
        repo: BOB,
        collection: post.$type,
        rkey: 'b1'
      }),
      expect.objectContaining({
        repo: BOB,
        collection: reaction.$type,
        rkey: 'b2'
      })
    ]);

    // A new record, an edited one, a deleted one.
    await w.write(ALICE, [{ collection: post.$type, rkey: 'a2', value: text('again') }]);
    await w.write(BOB, [
      { collection: post.$type, rkey: 'b1', value: text('hi, edited') },
      { collection: reaction.$type, rkey: 'b2' }
    ]);
    const changed = vi.fn();
    space.onChange(changed);

    await space.sync();
    expect(cursors(calls.writers)).toEqual([undefined, tid(2)]);
    expect(repos(calls.syncRepo)).toEqual([ALICE, BOB]);
    expect(calls.getRepoSnapshot).toHaveBeenCalledTimes(2);
    expect(values(space.records())).toEqual([
      [ALICE, post.$type, 'a1', { $type: post.$type, text: 'hello' }],
      [ALICE, post.$type, 'a2', { $type: post.$type, text: 'again' }],
      [BOB, post.$type, 'b1', { $type: post.$type, text: 'hi, edited' }]
    ]);
    expect(changed).toHaveBeenCalledOnce();
    expect(changed).toHaveBeenCalledWith(space.records());

    // Nothing moved: the writers are listed, and nothing pulled.
    await space.sync();
    expect(cursors(calls.writers)).toEqual([undefined, tid(2), tid(4)]);
    expect(calls.syncRepo).toHaveBeenCalledTimes(2);
    expect(changed).toHaveBeenCalledOnce();
  });

  it('drops the records of a writer removed from the set', async () => {
    const w = await world();
    const { space } = openRoom(w);
    await space.sync();
    const changed = vi.fn();
    const stop = space.onChange(changed);

    w.remove(BOB);
    await space.sync();
    expect(values(space.records())).toEqual([
      [ALICE, post.$type, 'a1', { $type: post.$type, text: 'hello' }]
    ]);
    expect(changed).toHaveBeenCalledOnce();

    stop();
    await w.write(CAROL, [{ collection: post.$type, rkey: 'c1', value: text('hey') }]);
    await space.sync();
    expect(space.records().map((r) => r.repo)).toEqual([ALICE, CAROL]);
    expect(changed).toHaveBeenCalledOnce();
  });

  it('pulls a repo in full when its log no longer reaches the kept head', async () => {
    const w = await world();
    const { space, calls } = openRoom(w);
    await space.sync();

    await w.write(BOB, [{ collection: post.$type, rkey: 'b3', value: text('later') }]);
    w.gaps.add(BOB);
    await space.sync();
    expect(repos(calls.syncRepo)).toEqual([BOB]);
    expect(repos(calls.getRepoSnapshot)).toEqual([ALICE, BOB, BOB]);
    expect(space.records({ collections: [post] }).map((r) => r.rkey)).toEqual(['a1', 'b1', 'b3']);
  });

  it('drops a writer whose repo its host no longer has or serves', async () => {
    const w = await world();
    await w.write(CAROL, [{ collection: post.$type, rkey: 'c1', value: text('hey') }]);
    const { space } = openRoom(w);
    await space.sync();

    await w.write(BOB, [{ collection: post.$type, rkey: 'b3', value: text('later') }]);
    w.repos.delete(BOB);
    await w.write(CAROL, [{ collection: post.$type, rkey: 'c2', value: text('again') }]);
    w.failing.set(CAROL, xrpcError('RepoTakendown'));
    await space.sync();
    expect(space.records().map((r) => r.repo)).toEqual([ALICE]);
  });

  it("keeps a failing writer's last records, the rest of the sync, and pulls it again", async () => {
    const w = await world();
    const store = memoryStore();
    const { space, calls } = openRoom(w, { store });
    await space.sync();

    await w.write(ALICE, [{ collection: post.$type, rkey: 'a2', value: text('again') }]);
    await w.write(BOB, [{ collection: post.$type, rkey: 'b3', value: text('later') }]);
    const down = new Error('host down');
    w.failing.set(BOB, down);
    expect(await space.sync()).toEqual({
      failed: [{ repo: BOB, error: down }]
    });
    // Alice's new record kept; Bob's old ones held.
    expect(space.records().map((r) => r.rkey)).toEqual(['a1', 'a2', 'b1', 'b2']);
    expect(JSON.parse(store.kept.get(`${ALICE} ${SPACE}`)!)).toMatchObject({
      cursor: tid(4),
      retry: [BOB]
    });

    // Bob has not moved since, but is pulled again.
    w.failing.delete(BOB);
    expect(await space.sync()).toEqual({ failed: [] });
    expect(cursors(calls.writers)).toEqual([undefined, tid(2), tid(4)]);
    expect(repos(calls.syncRepo)).toEqual([ALICE, BOB, BOB]);
    // By writer, then collection, then key.
    expect(space.records().map((r) => r.rkey)).toEqual(['a1', 'a2', 'b1', 'b3', 'b2']);
    expect(JSON.parse(store.kept.get(`${ALICE} ${SPACE}`)!).retry).toBeUndefined();
  });

  it('keeps nothing of a sync whose writers cannot be listed', async () => {
    const w = await world();
    const { space, calls } = openRoom(w);
    await space.sync();
    const before = space.records();

    await w.write(ALICE, [{ collection: post.$type, rkey: 'a2', value: text('again') }]);
    calls.writers.mockImplementationOnce(() => {
      throw new Error('host down');
    });
    await expect(space.sync()).rejects.toThrow('host down');
    expect(space.records()).toEqual(before);

    await space.sync();
    expect(cursors(calls.writers)).toEqual([undefined, tid(2), tid(2)]);
    expect(space.records().map((r) => r.rkey)).toContain('a2');
  });

  it('runs one sync at a time, a second call awaiting the first', async () => {
    const w = await world();
    const { space, calls } = openRoom(w);

    const first = space.sync();
    const second = space.sync();
    expect(second).toBe(first);
    await Promise.all([first, second]);
    expect(calls.writers).toHaveBeenCalledOnce();
    expect(calls.getRepoSnapshot).toHaveBeenCalledTimes(2);

    await space.sync();
    expect(calls.writers).toHaveBeenCalledTimes(2);
  });

  it('filters records by collection, by NSID or lexicon namespace', async () => {
    const { space } = openRoom(await world());
    await space.sync();

    expect(space.records({ collections: [reaction] }).map((r) => r.rkey)).toEqual(['b2']);
    expect(space.records({ collections: [post.$type] }).map((r) => r.rkey)).toEqual(['a1', 'b1']);
    expect(space.records({ collections: [] })).toEqual([]);
    expect(space.records()).toHaveLength(3);
  });

  it('keeps its heads and records in the store between visits', async () => {
    const w = await world();
    const store = memoryStore();
    const first = openRoom(w, { store });
    await first.space.sync();
    expect([...store.kept.keys()]).toEqual([`${ALICE} ${SPACE}`]);

    // A later visit starts from what was kept: nothing to pull.
    const later = openRoom(w, { store });
    await later.space.sync();
    expect(later.calls.getRepoSnapshot).not.toHaveBeenCalled();
    expect(cursors(later.calls.writers)).toEqual([tid(2)]);
    expect(later.space.records()).toEqual(first.space.records());

    // It reads on from the kept heads, its CIDs looked up from the store.
    await w.write(BOB, [{ collection: post.$type, rkey: 'b1', value: text('edited') }]);
    await later.space.sync();
    const [[{ head }]] = later.calls.syncRepo.mock.calls as [[SpaceReader.SyncRepoOptions]];
    expect(head).toEqual(headOf(w.repos.get(BOB)![0]!));
    expect(later.calls.getRepoSnapshot).not.toHaveBeenCalled();
    expect(later.space.records().find((r) => r.rkey === 'b1')?.value).toEqual({
      $type: post.$type,
      text: 'edited'
    });

    // What is kept of another version is read again in full.
    store.kept.set(`${ALICE} ${SPACE}`, JSON.stringify({ version: 0 }));
    const stale = openRoom(w, { store });
    await stale.space.sync();
    expect(repos(stale.calls.getRepoSnapshot)).toEqual([ALICE, BOB]);
  });

  it('lists every writer again once its cursor is older than removals are kept', async () => {
    vi.useFakeTimers({ toFake: ['Date'] });
    const w = await world();
    const { space, calls } = openRoom(w);
    await space.sync();

    w.remove(BOB);
    w.forget(BOB);
    vi.advanceTimersByTime(72 * 60 * 60 * 1000);
    await space.sync();
    expect(cursors(calls.writers)).toEqual([undefined, undefined]);
    expect(calls.syncRepo).not.toHaveBeenCalled();
    expect(space.records().map((r) => r.repo)).toEqual([ALICE]);
  });

  it("writes to the member's own repo in the space, through lex-space", async () => {
    const rkey = '3jzfcijpj2z2a';
    const sent: [string, unknown][] = [];
    const space = GroupSpace.open(
      room,
      { authority: GROUP },
      {
        agent: {
          did: ALICE,
          fetchHandler: async (path, init) => {
            sent.push([
              new URL(path, 'https://pds.example').pathname,
              JSON.parse(String(init.body))
            ]);
            const cid = (await cidForLex({})).toString();
            return Response.json({
              uri: `at://${ALICE}/${post.$type}/${rkey}`,
              cid
            });
          }
        }
      }
    );

    await space.create(post, { text: 'hello' }, { rkey });
    await space.put(post, { text: 'edited' }, { rkey });
    await space.delete(post, { rkey });
    const target = { space: SPACE, repo: ALICE, collection: post.$type, rkey };
    expect(sent).toEqual([
      [
        '/xrpc/com.atproto.space.createRecord',
        { ...target, record: { $type: post.$type, text: 'hello' } }
      ],
      [
        '/xrpc/com.atproto.space.putRecord',
        { ...target, record: { $type: post.$type, text: 'edited' } }
      ],
      ['/xrpc/com.atproto.space.deleteRecord', target]
    ]);
  });
});
