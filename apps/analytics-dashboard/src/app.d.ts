import type { AccessIdentity } from '#lib/server/access-jwt.js'

declare global {
  namespace App {
    interface Locals {
      /**
       * Who the verified Cloudflare Access JWT vouched for, set by `hooks.server.ts`. A person or a
       * service token; unset under `vite dev`, where the gate is compiled out.
       */
      identity?: AccessIdentity
    }
  }
}

export {}
