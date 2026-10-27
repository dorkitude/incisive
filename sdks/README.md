# incisive SDKs

Client libraries for the incisive analytics HTTP API (`/1.0/projects/{project_id}/...`). Each SDK
keeps the method names, arguments and wire format that existing analytics code in that language
already uses, so porting an app is usually a dependency rename plus a base URL. Every SDK
defaults to `http://localhost:8080` and API version `1.0`.

| Directory | Package | Tracking | Analyses | Saved queries | Datasets | Discovery / deletes | Access keys | Scoped keys |
|---|---|---|---|---|---|---|---|---|
| [`javascript/`](javascript) | `incisive-tracking`, `incisive-analysis`, `incisive-dataset` | fetch, Beacon API, image pixel, batching, auto-tracking | all | results + raw CRUD | results | raw requests | raw requests | - |
| [`python/`](python) | `incisive` | single, batch, image beacon URL | all except standard deviation | full | full | yes | full | yes |
| [`go/`](go) | `github.com/dorkitude/incisive/sdks/go` | single, batch, background batch client | all | full | full | yes | full | yes |
| [`ruby/`](ruby) | `incisive` gem | single, batch, async, beacon/redirect URLs | all | full | full | yes | full | yes |
| [`php/`](php) | `dorkitude/incisive` | single, batch | all except standard deviation | full | - | yes | - | yes |
| [`java/`](java) | `io.github.dorkitude.incisive:incisive-java` | single, async, queued batches | all except extraction | full | full | - | - | yes |
| [`dotnet/`](dotnet) | `Incisive` (NuGet) | single, batch, cached events, add-ons | all | full | full | collections | - | yes |

Gaps follow the original libraries for each language; see each README for the exact surface.

## Credentials

- **write key**: recording events
- **read key**: analyses, saved query results, dataset results, schema discovery
- **master key**: everything, including deletes, saved query/dataset definitions and access keys
- **scoped keys**: AES-256-CBC encrypted JSON (`hex(iv) + hex(ciphertext)`) minted from the master
  key, restricting operations and adding filters; every SDK that supports them produces keys the
  others can decrypt.

## Tests

Each SDK has unit tests that run against an in-process mock HTTP server and assert the exact
method, path, headers, query string and body, plus integration tests that run only when these are
set:

```sh
export INCISIVE_BASE_URL=http://localhost:8080
export INCISIVE_PROJECT_ID=...
export INCISIVE_WRITE_KEY=... INCISIVE_READ_KEY=... INCISIVE_MASTER_KEY=...
```

Run everything (Go and Python locally, the rest in pinned Docker images):

```sh
sdks/test-all.sh            # or: sdks/test-all.sh javascript ruby
```

Licensed under the MIT license in [`LICENSE`](LICENSE).
