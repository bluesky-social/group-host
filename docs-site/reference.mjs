// The reference pages read as reference, not as Markdown tables: a table
// whose rows are a name and what it does becomes a list of them, each name
// on a line of its own; a route or XRPC method shows its HTTP method as a
// badge; and an XRPC method of the host's own has its lexicon under it,
// parameters, input and output, read from lexicons/ so it cannot drift. An
// MCP tool has the lexicon its table's Lexicon column names: the method it
// asks, or the record it writes. Each method there is an anchor, its NSID,
// and a method named in code on any page, `org.atgrp.internal.addRecoveryKey` or
// `addRecoveryKey`, links to it.
// docs/ stays plain Markdown: all of this happens as the site is built.

import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const LEXICONS = join(here, '../lexicons');
import { REPO } from './plugins.mjs';

/** Every org.atgrp lexicon, the host's own internal ones too, by NSID. */
const lexicons = new Map(
  ['org/atgrp', 'org/atgrp/internal']
    .map((dir) => join(LEXICONS, dir))
    .filter((dir) => existsSync(dir))
    .flatMap((dir) =>
      readdirSync(dir)
        .filter((f) => f.endsWith('.json'))
        .map((f) => JSON.parse(readFileSync(join(dir, f), 'utf8')))
    )
    .map((l) => [l.id, l])
);

/** Every method the API page lists, by NSID; and by its name alone, where only one has that name. */
const API = join(here, '../docs/reference/api.md');
const listed = existsSync(API)
  ? new Set(
      [
        ...readFileSync(API, 'utf8').matchAll(
          /`(?:GET|POST) ((?:[a-z][a-z0-9-]*\.){2,}[a-zA-Z][a-zA-Z0-9]*)`/g
        )
      ].map((m) => m[1])
    )
  : new Set();
const short = new Map();
for (const nsid of listed) {
  const name = nsid.split('.').pop();
  short.set(name, short.has(name) ? null : nsid);
}
/** The NSID a span of code names, if it is a method the API page lists. */
const named = (code) => (listed.has(code) ? code : (short.get(code) ?? undefined));

const METHOD = { query: 'GET', procedure: 'POST', subscription: 'WS' };

const text = (node) =>
  node.type === 'text' || node.type === 'inlineCode'
    ? node.value
    : (node.children ?? []).map(text).join('');

const escape = (s) =>
  String(s)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');

/** `GET /refs`, `POST org.atgrp.x`, `org.atgrp.x?q=`: its method, and the NSID it names, if any. */
function route(code) {
  const m = code.match(/^(GET|POST|PUT|DELETE|PATCH|HEAD)\s+(\S+)/);
  const path = m ? m[2] : code;
  const nsid = path.match(/^(?:\/xrpc\/)?((?:[a-z][a-z0-9-]*\.){2,}[a-zA-Z][a-zA-Z0-9]*)/)?.[1];
  const lexicon = nsid && lexicons.get(nsid);
  const method = m?.[1] ?? (lexicon && METHOD[lexicon.defs.main?.type]);
  return { method, nsid: lexicon ? nsid : undefined };
}

/** A field's type, as one reads it: `string (did)`, `integer, 1 to 500, default 50`, `ref repoView`. */
function type(field) {
  if (!field) return '';
  const bits = [];
  switch (field.type) {
    case 'string':
      bits.push(field.format ? `string (${field.format})` : 'string');
      if (field.maxLength) bits.push(`at most ${field.maxLength} bytes`);
      if (field.knownValues) bits.push(field.knownValues.map((v) => `\`${v}\``).join(', '));
      break;
    case 'integer':
      bits.push('integer');
      if (field.minimum !== undefined && field.maximum !== undefined)
        bits.push(`${field.minimum} to ${field.maximum}`);
      break;
    case 'array':
      bits.push(`array of ${type(field.items)}`);
      if (field.maxLength) bits.push(`at most ${field.maxLength}`);
      break;
    case 'ref':
      bits.push(field.ref.replace(/^#/, '').replace(/^org\.atgrp\.(internal\.)?/, ''));
      break;
    case 'union':
      bits.push(
        `one of ${field.refs.map((r) => r.replace(/^#/, '').replace(/^org\.atgrp\.(internal\.)?/, '')).join(', ')}`
      );
      break;
    default:
      bits.push(field.type);
  }
  if (field.default !== undefined) bits.push(`default ${JSON.stringify(field.default)}`);
  return bits.join(', ');
}

/** Markdown's inline code, and nothing else, made HTML: enough for a lexicon's descriptions. */
const inline = (s) => escape(s ?? '').replace(/`([^`]+)`/g, '<code>$1</code>');

function fields(title, object) {
  if (!object) return '';
  if (object.type === 'ref' || object.type === 'union')
    return `<p class="lex-title">${title}</p><p class="lex-ref"><code>${escape(type(object))}</code></p>`;
  const props = Object.entries(object.properties ?? {});
  if (!props.length) return '';
  const required = new Set(object.required ?? []);
  const rows = props
    .map(
      ([name, f]) =>
        `<tr><td><code>${escape(name)}</code><span class="lex-type">${inline(type(f))}${required.has(name) ? '' : ', optional'}</span></td>` +
        `<td>${inline(f.description)}</td></tr>`
    )
    .join('');
  return `<p class="lex-title">${title}</p><table class="lex-fields"><tbody>${rows}</tbody></table>`;
}

/** The lexicon of `nsid`, folded away; named, among several. */
function lexicon(nsid, named = false) {
  const lex = lexicons.get(nsid);
  const main = lex.defs.main;
  const body =
    fields('Record', main.record) +
    fields('Parameters', main.parameters) +
    fields('Input', main.input?.schema) +
    fields('Output', main.output?.schema) +
    (main.errors?.length
      ? `<p class="lex-title">Errors</p><p>${main.errors.map((e) => `<code>${escape(e.name)}</code>`).join(', ')}</p>`
      : '');
  const source = `${REPO}/blob/main/lexicons/${nsid.replaceAll('.', '/')}.json`;
  return (
    `<details class="lexicon"><summary>Lexicon${named ? ` of <code>${escape(nsid.split('.').pop())}</code>` : ''}</summary>` +
    (main.description ? `<p>${inline(main.description)}</p>` : '') +
    body +
    `<p class="lex-source"><a href="${source}">${escape(nsid)}.json</a></p></details>`
  );
}

/** A heading's anchor as GitHub, and harness/docs.mjs, make it. */
const slug = (s) =>
  s
    .trim()
    .toLowerCase()
    .replace(/[^\p{L}\p{N}_\- ]/gu, '')
    .replaceAll(' ', '-');

const badge = (method) =>
  method && {
    type: 'methodBadge',
    data: {
      hName: 'span',
      hProperties: { className: ['method', `method-${method.toLowerCase()}`] }
    },
    children: [{ type: 'text', value: method }]
  };
const gap = { type: 'text', value: ' ' };

/** A command or route as code that breaks only between its words: a browser would break inside `--flag` after its hyphens. */
const signature = (code) => ({
  type: 'html',
  value: `<code>${code
    .split(/(\s+)/)
    .map((w) => (/\s/.test(w) ? w : `<span class="word">${escape(w)}</span>`))
    .join('')}</code>`
});

/** A cell's code, if the cell starts with some. */
const lead = (cell) => (cell.children[0]?.type === 'inlineCode' ? cell.children[0].value : null);

/** A method's term on the API page is its anchor, its NSID; once on a page, where it is listed twice. */
const anchored = new Set();
function anchor(code) {
  const nsid = code.match(/^(?:GET|POST)\s+(\S+)$/)?.[1];
  if (!nsid || !listed.has(nsid) || anchored.has(nsid)) return {};
  anchored.add(nsid);
  return { id: nsid };
}

/** A table of names, each with a sentence or more about it, rather than a grid of values. */
function definitional(table) {
  const [, ...rows] = table.children;
  if (!rows.length || !rows.every((r) => lead(r.children[0]) !== null)) return false;
  // Routes, methods and MCP tools read as reference, each with its lexicon.
  if (text(table.children[0].children[0]) === 'Tool') return true;
  if (rows.some((r) => route(lead(r.children[0])).method)) return true;
  // Four columns or more never fit the page's width; fewer do while the last is short.
  if (table.children[0].children.length >= 4) return true;
  const last = rows.map((r) => text(r.children.at(-1)).length);
  return last.reduce((a, b) => a + b, 0) / rows.length > 60;
}

/** The table as a list: each row's first cell its term, the middle cells labelled by their heads, the last its description. */
function asList(table) {
  const [head, ...rows] = table.children;
  const heads = head.children.map(text);
  const items = rows.flatMap((row) => {
    const [first, ...rest] = row.children;
    // A cell naming several routes, `GET a`, `GET b`: each its own term, with its badge.
    // An MCP tool, `member_assign`, is no route: its lexicon is the one its row's Lexicon cell names.
    const tool = heads[0] === 'Tool';
    const codes = first.children
      .filter((c) => c.type === 'inlineCode')
      .map((c) => ({ code: c.value, ...(tool ? {} : route(c.value)) }));
    const routes = tool || codes.some((c) => c.method);
    const terms = routes
      ? codes.map(({ code, method }) => ({
          type: 'refTerm',
          data: { hName: 'dt', hProperties: anchor(code) },
          children: [
            ...(method ? [badge(method), gap] : []),
            signature(code.replace(/^[A-Z]+\s+/, ''))
          ]
        }))
      : [
          {
            type: 'refTerm',
            data: { hName: 'dt' },
            children: [signature(first.children[0].value), ...first.children.slice(1)]
          }
        ];
    const named = row.children[heads.indexOf('Lexicon')]?.children ?? [];
    const nsids = tool
      ? named.filter((c) => c.type === 'inlineCode' && lexicons.has(c.value)).map((c) => c.value)
      : codes.map((c) => c.nsid).filter(Boolean);
    const middle = rest.slice(0, -1).flatMap((cell, i) =>
      text(cell).trim()
        ? [
            {
              type: 'refMeta',
              data: { hName: 'p', hProperties: { className: ['ref-meta'] } },
              children: [
                {
                  type: 'html',
                  value: `<span class="ref-label">${escape(heads[i + 1])}</span> `
                },
                ...cell.children
              ]
            }
          ]
        : []
    );
    // The last column is the description; labelled too when others are, so each says what it is.
    const label =
      rest.length > 1 && heads.at(-1)
        ? [{ type: 'html', value: `<span class="ref-label">${escape(heads.at(-1))}</span> ` }]
        : [];
    const description = {
      type: 'paragraph',
      children: [...label, ...(rest.at(-1)?.children ?? [])]
    };
    const detail = {
      type: 'refDetail',
      data: { hName: 'dd' },
      children: [
        ...middle,
        description,
        ...nsids.map((n) => ({ type: 'html', value: lexicon(n, nsids.length > 1) }))
      ]
    };
    return [...terms, detail];
  });
  return {
    type: 'refList',
    data: { hName: 'dl', hProperties: { className: ['ref-list'] } },
    children: items
  };
}

/** A method named in code, outside a link or heading, links to its entry on the API page. */
function linkMethods(node, api) {
  if (node.type === 'link' || node.type === 'heading' || node.type === 'refTerm') return;
  node.children?.forEach((child, i) => {
    const nsid = child.type === 'inlineCode' && named(child.value);
    if (nsid) node.children[i] = { type: 'link', url: `${api}#${nsid}`, children: [child] };
    else linkMethods(child, api);
  });
}

export function remarkReference({ base = '/' } = {}) {
  const api = `${base}reference/api/`;
  return (tree) => {
    anchored.clear();
    const visit = (parent) => {
      parent.children?.forEach((node, i) => {
        if (node.type === 'table' && definitional(node)) parent.children[i] = asList(node);
        else if (
          node.type === 'heading' &&
          node.children.length === 1 &&
          node.children[0].type === 'inlineCode'
        ) {
          // `### \`GET /healthz\``, `### \`org.atgrp.internal.getGroup\``: the method a badge before it. The
          // anchor is fixed first, from the heading as written, so links to it still land.
          const code = node.children[0];
          const { method, nsid } = route(code.value);
          if (!method) return;
          node.data = {
            ...node.data,
            hProperties: { ...node.data?.hProperties, id: slug(code.value) }
          };
          node.children = [
            badge(method),
            gap,
            { ...code, value: code.value.replace(/^[A-Z]+\s+/, '') }
          ];
          if (nsid) {
            // At the end of its section, after what is said of it.
            let end = i + 1;
            while (
              end < parent.children.length &&
              !(parent.children[end].type === 'heading' && parent.children[end].depth <= node.depth)
            )
              end++;
            parent.children.splice(end, 0, { type: 'html', value: lexicon(nsid) });
          }
        } else visit(node);
      });
    };
    visit(tree);
    linkMethods(tree, api);
  };
}
