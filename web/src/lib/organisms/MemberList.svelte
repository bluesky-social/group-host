<script lang="ts">
	import { goto } from '$app/navigation';
	import Empty from '#lib/molecules/Empty.svelte';
	import MemberRow from '#lib/molecules/MemberRow.svelte';
	type Member = { actor: { did: string; handle?: string }; roles: { role: string }[] };
	/**
	 * Who is in the group, `shown` at a time, found by part of their handle
	 * (`q`), with more to show while there are (`more`); and their roles:
	 * for one who may give roles, the ones they may give, to give or take;
	 * for one who may put people out, the way to. Their own row they leave
	 * from, not here.
	 */
	let {
		members,
		q = '',
		shown,
		more = false,
		me,
		assignable = [],
		canEject = false
	}: { members: Member[]; q?: string; shown: number; more?: boolean; me?: string; assignable?: string[]; canEject?: boolean } = $props();
	const named = (m: Member) => m.actor.handle ?? m.actor.did;
	// Found again as it is typed, the field keeping its place; sent as a form without a script.
	function find(e: Event) {
		const typed = (e.currentTarget as HTMLInputElement).value.trim();
		goto(typed ? `?q=${encodeURIComponent(typed)}` : '?', { reset: false, replace: true });
	}
</script>

<form class="find" method="get" role="search">
	<label class="field">Find a member <input name="q" type="search" value={q} autocomplete="off" placeholder="Part of their handle" oninput={find} /></label>
</form>
{#if members.length}
	<ul class="members">
		{#each members as m (m.actor.did)}
			<li class="member">
				<MemberRow actor={m.actor} roles={m.roles.map((r) => r.role)} />
				{#if m.actor.did !== me && (assignable.length || canEject)}
					<details class="manage">
						<summary>Change</summary>
						{#if assignable.length}
							<form method="post" action="?/roles">
								<input type="hidden" name="did" value={m.actor.did} />
								{#each m.roles.filter((r) => !assignable.includes(r.role)) as kept (kept.role)}
									<input type="hidden" name="role" value={kept.role} />
								{/each}
								{#each assignable as role (role)}
									<label><input type="checkbox" name="role" value={role} checked={m.roles.some((r) => r.role === role)} /> {role}</label>
								{/each}
								<button class="quiet" type="submit">Save {named(m)}'s roles</button>
							</form>
						{/if}
						{#if canEject}
							<form method="post" action="?/eject">
								<input type="hidden" name="did" value={m.actor.did} />
								<button class="quiet" type="submit">Put {named(m)} out</button>
							</form>
						{/if}
					</details>
				{/if}
			</li>
		{/each}
	</ul>
	{#if more}
		<form method="get" data-sveltekit-reset="false" data-sveltekit-replacestate>
			{#if q}<input type="hidden" name="q" value={q} />{/if}
			<input type="hidden" name="shown" value={shown + 5} />
			<button class="quiet" type="submit">Show more</button>
		</form>
	{/if}
{:else}
	<!-- A group always holds someone: none shown is none found. -->
	<Empty title="No member's handle has that in it">Try fewer letters of it, or clear the search to see everyone.</Empty>
{/if}
