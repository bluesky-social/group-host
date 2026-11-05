// Everything long: a group whose handle, name, words, links and rule
// run as long as the host takes them, dan's; maximilianaquinces in it,
// the longest handle the PDS takes; and frank asking to join with a
// note that wraps more than once.

export const GROUP = 'the-evening-society-of-ridge-walkers-and-lantern-keepers.groups.test'

export default async ({ opensocial, ask, who, exists }) => {
  const g = GROUP
  if (!(await exists(g))) opensocial('dan', 'group', 'create', g, '--join-policy', 'approval')
  const name = 'The Evening Society of Ridge Walkers and Lantern Keepers, North'
  const about =
    'We walk the long ridge every Thursday evening from the first warm week of April until the clocks go back, lanterns lit, at a pace anyone can keep. Bring water, a warm layer, and a lantern if you have one; we lend them to whoever does not. Dogs are welcome on a lead.'
  const links = [
    { url: 'https://a-very-long-domain-name-for-a-walking-group.example/routes/the-north-ridge/in-the-evening', title: 'Every route we walk, with its distances and where to park' },
    { url: 'https://chat.example' },
  ]
  opensocial('dan', 'profile', 'set', g, '--display-name', name, '--description', about, '--links', JSON.stringify(links))
  const rule = 'Wait for the last walker at every gate, stile and fork'
  opensocial('dan', 'rule', 'set', g, '--title', rule, '--text', 'Nobody walks on until everyone is through; the walker at the back carries the second lantern and says when all are there.')
  await ask('maximilianaquinces', g)
  opensocial('dan', 'member', 'assign', g, who.maximilianaquinces.handle, 'member')
  const note =
    'I have walked the ridge alone for years, mostly at dusk, and have a spare lantern or two. I would like to walk with others, and can lead the slower group if that helps anyone.'
  await ask('frank', g, note)
}
