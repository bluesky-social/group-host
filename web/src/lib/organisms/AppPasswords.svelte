<script lang="ts">
	import ConfirmDialog from '#lib/molecules/ConfirmDialog.svelte';
	import Empty from '#lib/molecules/Empty.svelte';
	import FormDialog from '#lib/molecules/FormDialog.svelte';
	import { day } from '#lib/dates.ts';
	/**
	 * The group's app passwords, never the passwords, each to revoke (asked
	 * to be sure of); a new one, shown once (`made`); and the dialog making
	 * one, opened by a button whose `popovertarget` is `make-password`.
	 */
	type Password = { name: string; member: string; privileged: boolean; lastUsedAt?: string };
	let { passwords, made }: { passwords: Password[]; made?: { name: string; password: string } } = $props();
</script>

{#if made}
	<p class="made">The app password “{made.name}” is <code class="password">{made.password}</code>. Copy it now: it is not shown again.</p>
{/if}
{#if passwords.length}
	<ul class="passwords">
		{#each passwords as p, i (p.name)}
			<li>
				<span class="row-main">
					<strong>{p.name}</strong>
					<span class="what">By <span class="mono">{p.member}</span>{p.privileged ? ', with chat' : ''}{p.lastUsedAt ? `; last used ${day(p.lastUsedAt)}` : ''}</span>
				</span>
				<span class="row-actions"><button class="quiet small" type="button" popovertarget={`unpassword-${i}`} aria-label={`Revoke “${p.name}”`}>Revoke</button></span>
				<ConfirmDialog id={`unpassword-${i}`} title={`Revoke “${p.name}”?`} action="?/unpassword" confirm="Revoke it" fields={{ name: p.name }}>
					Whatever signed in with it is signed out, and it signs in no more.
				</ConfirmDialog>
			</li>
		{/each}
	</ul>
{:else}
	<Empty title="No app passwords">An app that signs in with a handle and a password, as the Bluesky app does, signs in to the group with one.</Empty>
{/if}
<FormDialog id="make-password" title="Make an app password" action="?/password" submit="Make it">
	{#snippet intro()}It is shown once, when it is made.{/snippet}
	<label class="field">Where it is used <input name="name" required maxlength="64" placeholder="My phone" /></label>
	<label><input type="checkbox" name="privileged" /> It may use chat</label>
</FormDialog>
