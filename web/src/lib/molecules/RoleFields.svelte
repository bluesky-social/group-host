<script lang="ts">
  import { m } from '#lib/paraglide/messages.js';
  import { SCOPES, SETS, SINGLE, type SetKey } from '#lib/actions.ts';
  /**
   * What a role may do, for a form making or changing it: the group's
   * permission sets, each its own choice, then the actions one by one,
   * then the roles its holders may give and whether members get it on
   * joining. `scopes` are the role's as it is (none for a new one), each
   * with the actions the host reads it as granting.
   *
   * What a ticked set gives shows ticked and greyed, saying which set it
   * comes with, and cannot be unticked: to take one of those away is to
   * choose a smaller set, or none, and tick the rest one by one. Ticked
   * on its own too, it stays so beneath, and shows once the set goes. So
   * is what the role's other scopes give, which the form leaves as they
   * are. Offered
   * are only what the one editing may give (`grantable`, the scopes the
   * host says they hold enough of; `mine`, the roles they may give, `*`
   * for every one) and what the role has already, to take away.
   */
  type Role = { key: string; name?: string };
  type Scope = { scope: string; actions?: string[] };
  let {
    scopes = [],
    assignable = [],
    roles,
    grantable,
    mine,
    isDefault = false
  }: {
    scopes?: Scope[];
    assignable?: string[];
    roles: Role[];
    grantable: string[];
    mine: string[];
    isDefault?: boolean;
  } = $props();
  const had = $derived(scopes.map((s) => s.scope));
  const named = $derived(new Set([...Object.values(SCOPES), ...SETS.map((s) => s.scope)]));
  // What its scopes the form does not show give.
  const others = $derived(
    new Set(scopes.filter((s) => !named.has(s.scope)).flatMap((s) => s.actions ?? []))
  );
  // Each set and action as ticked by hand: as the role has it, until changed.
  let sets = $derived(SETS.filter((s) => had.includes(s.scope)).map((s) => s.key));
  let picked = $derived(SINGLE.filter((a) => had.includes(SCOPES[a as keyof typeof SCOPES])));
  const toggle = <T>(list: T[], value: T, on: boolean) =>
    on ? [...list, value] : list.filter((v) => v !== value);
  // The set a set comes with, ticked.
  const within = (key: SetKey) =>
    SETS.find((s) => sets.includes(s.key) && s.within.includes(key))?.key;
  const ticked = $derived(SETS.filter((s) => sets.includes(s.key) && !within(s.key)));
  // Where an action comes from: the largest set ticked that gives it, or
  // the role's other scopes.
  const from = (action: string): SetKey | 'other' | undefined =>
    ticked.findLast((s) => s.actions.includes(action))?.key ??
    (others.has(action) ? 'other' : undefined);
  const offered = (scope: string) => grantable.includes(scope) || had.includes(scope);
  const shownSets = $derived(SETS.filter((s) => offered(s.scope) || within(s.key)));
  const shownActions = $derived(
    SINGLE.filter((a) => offered(SCOPES[a as keyof typeof SCOPES]) || from(a))
  );
  const gives = (r: string) => mine.includes('*') || mine.includes(r) || assignable.includes(r);
  const setNames: Record<SetKey, () => string> = {
    profile: m.role_fields_set_profile,
    moderate: m.role_fields_set_moderate,
    group: m.role_fields_set_group
  };
  const setLines: Record<SetKey, () => string> = {
    profile: m.role_fields_set_profile_line,
    moderate: m.role_fields_set_moderate_line,
    group: m.role_fields_set_group_line
  };
  const comesWith: Record<SetKey | 'other', () => string> = {
    profile: m.role_fields_with_profile,
    moderate: m.role_fields_with_moderate,
    group: m.role_fields_with_group,
    other: m.role_fields_with_other
  };
</script>

{#if shownSets.length}
  <fieldset class="choices">
    <legend>{m.role_fields_sets()}</legend>
    {#each shownSets as s (s.key)}
      {@const by = within(s.key)}
      {#if by && sets.includes(s.key)}<input type="hidden" name="set" value={s.key} />{/if}
      <label
        ><input
          type="checkbox"
          name="set"
          value={s.key}
          checked={!!by || sets.includes(s.key)}
          disabled={!!by}
          onchange={(e) => (sets = toggle(sets, s.key, e.currentTarget.checked))}
        />
        <span class="described"
          ><span>{setNames[s.key]()}</span>
          <span class="mute small">{setLines[s.key]()}</span>
          {#if by}<span class="mute small">{comesWith[by]()}</span>{/if}</span
        ></label
      >
    {/each}
  </fieldset>
{/if}
{#if shownActions.length}
  <fieldset class="choices">
    <legend>{m.role_fields_may()}</legend>
    {#each shownActions as a (a)}
      {@const by = from(a)}
      {#if by && picked.includes(a)}<input type="hidden" name="action" value={a} />{/if}
      <label
        ><input
          type="checkbox"
          name="action"
          value={a}
          checked={!!by || picked.includes(a)}
          disabled={!!by}
          onchange={(e) => (picked = toggle(picked, a, e.currentTarget.checked))}
        />
        {a}
        {#if by}<span class="mute small">{comesWith[by]()}</span>{/if}</label
      >
    {/each}
  </fieldset>
{/if}
<fieldset>
  <legend>{m.role_fields_can_give()}</legend>
  {#each roles.filter((r) => gives(r.key)) as r (r.key)}<label
      ><input
        type="checkbox"
        name="assignable"
        value={r.key}
        checked={assignable.includes(r.key)}
      />
      {r.name ?? r.key}</label
    >{/each}
  {#if gives('*')}<label
      ><input type="checkbox" name="assignable" value="*" checked={assignable.includes('*')} />
      {m.role_fields_any_role()}</label
    >{/if}
</fieldset>
<label><input type="checkbox" name="default" checked={isDefault} /> {m.role_fields_default()}</label
>
