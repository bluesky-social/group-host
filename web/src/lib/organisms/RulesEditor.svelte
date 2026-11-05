<script lang="ts">
	import ConfirmDialog from '#lib/molecules/ConfirmDialog.svelte';
	import Empty from '#lib/molecules/Empty.svelte';
	import FormDialog from '#lib/molecules/FormDialog.svelte';
	/**
	 * A group's rules, in order, each to delete (asked to be sure of);
	 * and the dialog adding one, opened by a button whose `popovertarget`
	 * is `add-rule`.
	 */
	let { rules = [] }: { rules?: { rkey: string; title: string; text?: string }[] } = $props();
</script>

{#if rules.length}
	<ul class="rule-rows">
		{#each rules as r, i (r.rkey)}
			<li>
				<span class="ordinal">{i + 1}</span>
				<span class="row-main"><strong>{r.title}</strong>{#if r.text}<span class="what">{r.text}</span>{/if}</span>
				<span class="row-actions"><button class="quiet small" type="button" popovertarget={`unrule-${i}`} aria-label={`Delete “${r.title}”`}>Delete</button></span>
				<ConfirmDialog id={`unrule-${i}`} title="Delete this rule?" action="?/unrule" confirm="Delete" fields={{ rkey: r.rkey }}>
					“{r.title}” will no longer be one of the group's rules, here or in other apps.
				</ConfirmDialog>
			</li>
		{/each}
	</ul>
{:else}
	<Empty title="No rules yet">A rule says what everyone in the group agrees to.</Empty>
{/if}
<FormDialog id="add-rule" title="Add a rule" action="?/rule" submit="Add">
	<label class="field">Rule <input name="title" required maxlength="64" placeholder="Be kind" /></label>
	<label class="field">What it means <textarea name="text" maxlength="1000"></textarea></label>
</FormDialog>
