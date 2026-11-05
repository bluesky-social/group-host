// An https front for the console under test, at the harness's console
// hostname, passing every request on to it over plain http, as knotless's.
//
//   node harness/tls-proxy.mjs <cert> <key> <target port>
//
// Prints the port it listens on, and exits when stdin closes.

import { readFileSync } from 'node:fs'
import { createServer } from 'node:https'
import { request } from 'node:http'
import { connect } from 'node:net'

const [cert, key, target] = process.argv.slice(2)

const server = createServer({ cert: readFileSync(cert), key: readFileSync(key) }, (req, res) => {
  const upstream = request(
    { host: '127.0.0.1', port: target, path: req.url, method: req.method, headers: req.headers },
    (answer) => {
      res.writeHead(answer.statusCode, answer.headers)
      answer.pipe(res)
    },
  )
  upstream.on('error', () => res.writeHead(502).end())
  req.pipe(upstream)
})

// A WebSocket passed on as it is: the dev server's, which tells a page
// it is still there (harness/look serves the console so).
server.on('upgrade', (req, socket, head) => {
  const upstream = connect(Number(target), '127.0.0.1', () => {
    const headers = Object.entries(req.headers).map(([k, v]) => `${k}: ${v}`).join('\r\n')
    upstream.write(`${req.method} ${req.url} HTTP/1.1\r\n${headers}\r\n\r\n`)
    upstream.write(head)
    socket.pipe(upstream).pipe(socket)
  })
  upstream.on('error', () => socket.destroy())
  socket.on('error', () => upstream.destroy())
})

server.listen(0, '127.0.0.1', () => console.log(server.address().port))
process.stdin.on('end', () => process.exit(0))
process.stdin.on('close', () => process.exit(0))
process.stdin.resume()
