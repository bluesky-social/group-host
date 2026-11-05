<script lang="ts">
	import ConfirmDialog from '#lib/molecules/ConfirmDialog.svelte';
	import Empty from '#lib/molecules/Empty.svelte';
	import FormDialog from '#lib/molecules/FormDialog.svelte';
	import Person from '#lib/molecules/Person.svelte';
	import { day } from '#lib/dates.ts';
	/**
	 * The group's app passwords, never the passwords, each to revoke (asked
	 * to be sure of); a new one, shown once (`made`); and the dialog making
	 * one, opened by a button whose `popovertarget` is `make-password`.
	 */
	type Password = { name: string; member: string; actor?: { did: string; handle?: string }; privileged: boolean; lastUsedAt?: string };
	let { passwords, made }: { passwords: Password[]; made?: { name: string; password: string } } = $props();
</script>

{#if made}
	<p class="made">The app password “{made.name}” is <code class="password">{made.password}</code>. Copy it now. You won't see it again.</p>
{/if}
{#if passwords.length}
	<ul class="passwords">
		{#each passwords as p, i (p.name)}
			<li>
				<span class="row-main">
					<strong>{p.name}</strong>
					<span class="what">By <Person actor={p.actor ?? { did: p.member }} />{p.privileged ? ', with chat' : ''}{p.lastUsedAt ? `; last used ${day(p.lastUsedAt)}` : ''}</span>
				</span>
				<span class="row-actions"><button class="quiet small" type="button" popovertarget={`unpassword-${i}`} aria-label={`Revoke “${p.name}”`}>Revoke</button></span>
				<ConfirmDialog id={`unpassword-${i}`} title="Revoke this app password?" action="?/unpassword" confirm="Revoke" fields={{ name: p.name }}>
					Apps using “{p.name}” are signed out, and it stops working.
				</ConfirmDialog>
			</li>
		{/each}
	</ul>
{:else}
	<Empty title="No app passwords">Create one for an app that signs in with a handle and password, like the Bluesky app.</Empty>
{/if}
<FormDialog id="make-password" title="Create an app password" action="?/password" submit="Create">
	{#snippet intro()}You'll see it once, right after it's created.{/snippet}
	<label class="field">Where it is used <input name="name" required maxlength="64" placeholder="My phone" /></label>
	<label><input type="checkbox" name="privileged" /> It may use chat</label>
</FormDialog>
