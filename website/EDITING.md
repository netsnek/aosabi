# Editing the documentation on the site

This file designs editing the documentation where it is read, on the docs section of the site, as
in a wiki. Nothing of it is built yet beyond the first step, the link to the editor of GitHub that
every page carries today. Section 7 says what the rest takes to build, and section 8 names the
decisions that are still open.

## 1. The concept

A page of `/docs/` opens in reading mode, the static page that `assets-src/build_docs.py` renders
from a file of `Documentation/`. An Edit toggle on the page turns the same page into an editor in
place: the head of the page, its navigation and its footer stay, and the article becomes an MDX
editor with an Editor tab over the Markdown source and a Preview tab in the look of the site. The
toggle then reads Stop editing and leads back to reading mode, and leaving without saving discards
the change.

Saving publishes at once. There are no drafts, because a draft that nobody publishes is a page that
nobody reads. A save is one commit to the file in `Documentation/` on the branch `main`, made in
the name of the person who saved, so the history of a page is the history of its file in git, and
the site shows the new text after its next build. The editor of GitHub stays a second way to the
same files, and both ways commit to the same place.

## 2. The principles

1. Security comes before features, so a person edits only with a GitHub login and the write
   permission they already hold on the repository, the server keeps their token, and the editor
   touches Markdown files of `Documentation/` and nothing else.
2. Boilerplate comes before magic, so the store is git and nothing else, with no database, no second
   copy of a page and no sync job. What a page shows is its file on `main`, rendered by one builder.
3. Reading stays static. A page of `/docs/` carries no script, no cookie and no request to another
   origin, as every page of the site does today. Only the editor is a page with a script, and only
   a person who chose to edit loads it.
4. The API of the editor is GraphQL, served by Aosabi. The two routes of the GitHub login are the
   one exception, because GitHub sends the browser back with a plain request.

## 3. What exists

Every page of the docs section carries the link Edit on GitHub, which opens its file in the editor
of GitHub, and the link History, which lists the commits that changed it. The link already works
for everybody with a GitHub account. A person with write access commits directly, and anybody else
gets a fork and a pull request from GitHub, which is how a change from outside the project arrives
today and stays.

## 4. The pieces

### 4.1 The toggle

The builder gains one option that names the address of the editor. With it, the Edit control of a
page leads to `/edit/docs/<page>/`, and the link Edit on GitHub stays beside it for a person without
write access. The control is a link and not a button, so the reading page needs no script.

### 4.2 The editor page

The Worker of section 4.3 answers `/edit/docs/<page>/` with the static page of `/docs/<page>/`,
fetched from the site on the same origin and rewritten as it streams through: the article is
replaced by the root element of the editor, and one script and one stylesheet of the editor are
added. A person who is not signed in sees the page in reading mode with the button Sign in with
GitHub and the link Edit on GitHub. A signed in person without write permission sees the same page
with a sentence that says so.

The editor is an MDX editor limited to Markdown: CodeMirror over the source, with Markdown syntax
highlighting and a toolbar that inserts a heading, a list, a table, a code block and a link to
another page of the documentation, and a Preview tab that renders GitHub Flavored Markdown with the
rules of `docs.css`. The files stay Markdown without components and without raw HTML, because
GitHub renders the same files and the builder of the site shows every tag as text. The preview is a
preview, and the page that readers see is always the one `build_docs.py` renders after the commit.

Saving asks for a commit message, filled in with the path of the page, and sends the text with the
version it was loaded from. The answer names the commit and links to it, and the editor says that
the page changes with the next build of the site.

### 4.3 The Worker

The Worker is a small Aosabi service built for Cloudflare Workers, the Serverless pillar of Aosabi.
It runs in the Pages project of the site as the Worker of that project, and the file `_routes.json`
of the project hands it the path `/edit/*` alone, so the pages of the site stay static files that
never pass through it. The editor shares the origin of the docs, which keeps the cookie and every
request on one origin with no CORS. The Worker serves four routes:

1. The route `GET /edit/login` sends the browser to the login of GitHub with a state value and
   PKCE.
2. The route `GET /edit/callback` takes the code back, exchanges it for a user access token and
   sets the session cookie.
3. The route `POST /edit/graphql` is the GraphQL API below.
4. The route `GET /edit/docs/*` serves the editor page of section 4.2, and the script and the
   stylesheet of the editor come from its own assets.

The schema holds what the editor needs and no more:

```graphql
type Query {
  "The person signed in, or null."
  viewer: Viewer
  "A file of Documentation/ as it stands on main."
  document(path: String!): Document!
}

type Mutation {
  "Writes one file of Documentation/ as one commit on main."
  saveDocument(input: SaveDocumentInput!): Commit!
}

type Viewer {
  login: String!
  name: String
  canWrite: Boolean!
}

type Document {
  path: String!
  text: String!
  "The blob the text was read from, null for a page that does not exist yet."
  blob: String
  "The commit of main the text was read at."
  head: String!
}

input SaveDocumentInput {
  path: String!
  text: String!
  blob: String
  head: String!
  message: String!
}

type Commit {
  oid: String!
  url: String!
}
```

An error carries one code in its extensions: `UNAUTHENTICATED` without a session, `FORBIDDEN`
without write permission, `BAD_PATH` for a path outside the rules of section 5, `TOO_LARGE` for a
text over the limit, and `CONFLICT` when the file changed since the editor loaded it.

### 4.4 GitHub

The login is a GitHub App of the organization, installed on this repository alone, with the
permissions Contents read and write and Metadata read. The Worker signs a person in with the web
flow of the app and receives a user access token, which GitHub limits to the permissions of the app
and to what the person may do on the repository, and which expires after eight hours by GitHub's
default for such tokens. The Worker then speaks to GitHub through its GraphQL API alone:

1. The field `repository.viewerPermission` tells whether the person may write, which means
   `WRITE`, `MAINTAIN` or `ADMIN`.
2. The field `repository.object` with the expression `main:Documentation/<path>` reads the text
   and the blob of a file, and the field `repository.ref` reads the head of `main`.
3. The mutation `createCommitOnBranch` writes the file as one commit on `main`, with
   `expectedHeadOid` set to the head the editor loaded. GitHub makes the commit in the name of the
   person whose token it is.

When `main` moved since the editor loaded the file, `createCommitOnBranch` refuses. The Worker then
reads the blob of the file at the new head: when it is still the blob the editor loaded, nobody else
changed this file, and the Worker writes once more on the new head. When the blob differs, it
answers `CONFLICT`, and the editor shows both texts side by side, so the person merges by hand and
saves again. No change is ever lost and nothing is ever forced.

When `main` is protected so that every change needs a pull request, the Worker makes a branch of
its own, `docs/<login>/<page>`, with `createRef`, writes the commit there with
`createCommitOnBranch` and opens the pull request with `createPullRequest`, so the same editor
works under either rule of the repository. The permissions of the app then include Pull requests
read and write.

### 4.5 The build after a save

A workflow of the repository builds the site and deploys `public/` to Cloudflare Pages on every
push to `main` that changes `Documentation/` or `website/`, with a Cloudflare token that may deploy
this one Pages project and nothing else. A save on the site and a commit on GitHub reach the site
the same way, and the site never holds a page that is not on `main`.

## 5. Security

1. The session is a cookie named `__Secure-aosabi-edit` with the path `/edit/`, `HttpOnly`,
   `Secure` and `SameSite=Strict`. It holds the user access token encrypted with AES-GCM under a key
   that only the Worker holds, and it ends with the token, after eight hours at most. The token
   never reaches the script of the editor.
2. The GraphQL endpoint accepts a POST with the type `application/json` and an `Origin` header equal
   to the origin of the site and nothing else, which together with the cookie rule keeps another
   site from sending a request in the name of a signed in person.
3. A path must be a Markdown file under `Documentation/`: the pattern
   `^Documentation/[A-Za-z0-9._/-]+\.md$`, no segment that starts with a dot, no `..`, and no
   symbolic link. A text must be UTF-8 with at most 1 MiB, without a NUL byte, and is stored with
   line feeds alone.
4. The editor page carries a Content Security Policy that allows a script, a stylesheet and a
   request from its own origin alone, and no inline script and no frame.
5. The Worker logs no token, no cookie and no text of a page, and the site keeps setting no cookie
   outside `/edit/`.
6. The editor bundle is built by the workflow from a lockfile with fixed versions, and it is served
   from the origin of the site, never from a third party.
7. Whatever a person with write permission saves, a page of the site cannot run it: the builder
   shows every tag as text and links nothing but http, https and mailto addresses.

## 6. What it does not do

The first version edits existing pages and creates new ones under `Documentation/`. It does not
delete or move a page, upload an image, keep a draft, hold comments or search, and each of these
stays possible through GitHub. It does not render a page on the server: the only renderer of the
site stays `build_docs.py`.

## 7. What it takes to build

The estimates below are estimates of working days by one person and nothing measured.

1. The toggle: the option of `build_docs.py` for the address of the editor, and the line Last
   changed on every page, the author and the date of the last commit of its file, read from git when
   the site is built from a clone. Half a day.
2. The deploy workflow of section 4.5, with the Cloudflare token and the build of the editor
   bundle. Half a day.
3. The GitHub App: its name, its permissions, its callback address and its installation on this
   repository, and its client secret and the session key as secrets of the Worker. Half a day, and
   the decision of section 8 on who may save.
4. The Worker: the schema of section 4.3, the login, the session, the client of the GraphQL API of
   GitHub, the conflict rule, the checks of section 5 and the rewritten editor page. Two to three
   days.
5. The editor: CodeMirror with the toolbar, the preview, the side by side view of a conflict and
   the save. Two days.
6. The tests, a Jupyter notebook in Python as every test of the repository: the Worker against a
   scratch repository and a test account, with a read, a save, a new page, a conflict on the same
   file, a retry after a change to another file, a refusal without write permission, every path
   rule, the cookie flags, the origin rule, and the proof that no page of `/docs/` carries a script.
   One to two days.

That is about seven to nine working days in all, estimated as above.

## 8. Open decisions

1. Who saves directly to `main`: everybody with write permission on the repository, or a smaller
   group of maintainers of the documentation, with a pull request for everybody else.
2. Whether a save made on the site carries a `Signed-off-by` line in the name of the person, as a
   commit from a clone does, and so whether the editor asks the person to certify it.
3. The address of the editor, `/edit/` on the origin of the site in this design, and whether the
   Worker runs in the Pages project of the site or as a Worker of its own on that route.
4. Which packages the editor bundle may use. CodeMirror and a renderer of GitHub Flavored Markdown
   for the preview are the least it needs, and they are packages of the site alone, which no build
   of Aosabi ever contains.
