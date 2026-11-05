<script lang="ts">
	import Dialog from '#lib/molecules/Dialog.svelte';
	import Empty from '#lib/molecules/Empty.svelte';
	import { when } from '#lib/dates.ts';
	type Said = { $type?: string; reasonType?: string; reason?: string; status?: string; note?: string; val?: string; deleted?: boolean };
	type Event = { id: string; createdAt: string; createdBy?: string; event: Said };
	type Subject = { key: string; status: string; reportCount: number; events: Event[] };
	/**
	 * A group's moderation queue of one status: each subject, where it
	 * stands, what happened to it; for one who may, resolved or escalated,
	 * with a note, from a dialog.
	 */
	let { subjects, status, canResolve }: { subjects: Subject[]; status: string; canResolve: boolean } = $props();
	/** An event in a line: what it was, and what was said with it. */
	function said(e: Said): string {
		const kind = (e.$type ?? '').split('#')[1] ?? '';
		if (kind === 'reportEvent') return ['Reported', [e.reasonType?.split('#')[1], e.reason].filter(Boolean).join(': ')].filter(Boolean).join(': ');
		if (kind === 'statusEvent') return [e.status === 'resolved' ? 'Resolved' : e.status === 'escalated' ? 'Escalated' : (e.status ?? ''), e.note].filter(Boolean).join(': ');
		if (kind === 'labelEvent') return `${e.deleted ? 'Label removed' : 'Labelled'}: ${e.val}`;
		if (kind === 'ejectEvent') return 'Removed';
		return kind;
	}
</script>

{#if subjects.length}
	<ul class="queue">
		{#each subjects as s, i (s.key)}
			<li class="subject">
				<span class="row-main">
					<span class="mono who">{s.key}</span>
					<span class="what">{s.reportCount ? `Reported ${s.reportCount === 1 ? 'once' : `${s.reportCount} times`}` : 'Not reported'}</span>
				</span>
				{#if canResolve}
					<span class="row-actions"><button class="quiet small" type="button" popovertarget={`resolve-${i}`}>Resolve</button></span>
					<Dialog id={`resolve-${i}`} title="Resolve or escalate">
						<p class="mute mono">{s.key}</p>
						<form class="stack" method="post" action="?/resolve">
							<input type="hidden" name="subject" value={s.key} />
							<label class="field">A note, if you like <input name="note" maxlength="2000" /></label>
							<div class="actions">
								<button class="quiet" type="submit" name="status" value="escalated">Escalate</button>
								<button type="submit" name="status" value="resolved">Resolve</button>
							</div>
						</form>
					</Dialog>
				{/if}
				<ol class="events">
					{#each s.events as e (e.id)}<li><span class="what">{when(e.createdAt)}</span> {said(e.event)}</li>{/each}
				</ol>
			</li>
		{/each}
	</ul>
{:else}
	<Empty title={`Nothing ${status}`}>Anything {status} in the queue shows up here.</Empty>
{/if}
