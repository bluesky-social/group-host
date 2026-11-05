<script lang="ts">
	import ConfirmDialog from '#lib/molecules/ConfirmDialog.svelte';
	import FormDialog from '#lib/molecules/FormDialog.svelte';
	import Tag from '#lib/atoms/Tag.svelte';
	/**
	 * A group's roles: each, what it may do, to take away (asked to be sure
	 * of); and the dialog defining a new one, opened by a button whose
	 * `popovertarget` is `define-role`: what it may do, the roles its
	 * holders may give, and whether members get it on joining.
	 */
	type Role = { key: string; name?: string; description?: string };
	type Binding = { role: string; actions: string[] };
	let { roles = [], bindings = [], actions }: { roles?: Role[]; bindings?: Binding[]; actions: string[] } = $props();
	const may = (key: string) => bindings.find((b) => b.role === key)?.actions ?? [];
</script>

<ul class="roles">
	{#each roles as r, i (r.key)}
		<li>
			<span class="row-main">
				<span><strong>{r.name ?? r.key}</strong> <span class="mute mono">{r.key}</span></span>
				<span class="what">{may(r.key).length ? 'May' : 'Nothing beyond being a member'}</span>
				{#if may(r.key).length}<span class="tags">{#each may(r.key) as a (a)}<Tag text={a} />{/each}</span>{/if}
			</span>
			{#if r.key !== 'member'}
				<span class="row-actions"><button class="quiet small" type="button" popovertarget={`unrole-${i}`} aria-label={`Take away ${r.name ?? r.key}`}>Take away</button></span>
				<ConfirmDialog id={`unrole-${i}`} title={`Take away ${r.name ?? r.key}?`} action="?/unrole" confirm="Take it away" fields={{ role: r.key }}>
					The role goes, and what it lets its holders do with it. Only a role nobody holds can be taken away.
				</ConfirmDialog>
			{/if}
		</li>
	{/each}
</ul>
<FormDialog id="define-role" title="Define a role" action="?/role" submit="Define the role" wide>
	{#snippet intro()}What its holders may do, and whom they may give roles to. Members are given it from the group's page.{/snippet}
	<div class="link-rows">
		<label class="field">Its key <input name="role" required pattern="[a-z0-9-]+" maxlength="64" placeholder="moderator" /></label>
		<label class="field">Its name <input name="name" required maxlength="64" placeholder="Moderator" /></label>
	</div>
	<fieldset>
		<legend>It may</legend>
		{#each actions as a (a)}<label><input type="checkbox" name="action" value={a} /> {a}</label>{/each}
	</fieldset>
	<fieldset>
		<legend>Its holders may give</legend>
		{#each roles as r (r.key)}<label><input type="checkbox" name="assignable" value={r.key} /> {r.name ?? r.key}</label>{/each}
	</fieldset>
	<label><input type="checkbox" name="default" /> Every new member gets it</label>
</FormDialog>
