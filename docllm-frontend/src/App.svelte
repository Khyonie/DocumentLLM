<script lang="ts">
  import { onMount } from 'svelte';
  import { marked } from 'marked';
  import DOMPurify from "dompurify";

  /*
    A Svelte component has three possible regions:

    1. <script>  : state, types, and functions for this component.
    2. markup    : the HTML-like template below this script block.
    3. <style>   : optional component-local CSS.

    This app keeps shared/global styling in src/app.css instead of using a
    component-local <style> block, so layout and behavior stay separate.
  */

  /*
    This is the small message shape our frontend cares about.

    The backend's /v1/chat/completions endpoint already accepts OpenAI-like
    messages, so this type mirrors the useful subset of that format.
  */
  type ChatMessage = {
    role: 'user' | 'assistant'
    content: string
  }

  /*
    These response types describe the JSON we expect back from the server.

    TypeScript only knows that response.json() returns "some JavaScript value",
    so we give that value a shape before reading fields from it.
  */
  type ModelListResponse = {
    data?: Array<{ id: string }>
  }

  type ChatStreamChunk = {
    choices?: Array<{
      delta?: {
        content?: string
      }
      finish_reason?: string | null
    }>
  }

  type ErrorResponse = {
    error?: {
      message?: string
    }
  }

  type UploadedDocument = {
    id: string
    filename: string
  }

  type UploadListResponse = {
    documents?: UploadedDocument[]
  }

  type UploadResponse = {
    documents?: UploadedDocument[]
  }

  type IngestResponse = {
    documents?: UploadedDocument[]
    chunk_count?: number
  }

  /*
    $state(...) is Svelte 5's reactive state primitive.

    When one of these variables changes, Svelte automatically updates any part
    of the rendered DOM that reads that variable.
  */
  let models = $state<string[]>([])
  let selectedModel = $state('')
  let prompt = $state('')
  let messages = $state<ChatMessage[]>([])
  let selectedFiles = $state<File[]>([])
  let uploadedDocuments = $state<UploadedDocument[]>([])
  let selectedDocumentIds = $state<string[]>([])

  /*
    UI flags make the interface reflect in-flight async work.

    For example, while isSending is true, the Send button is disabled and its
    text changes to "Sending...".
  */
  let isLoadingModels = $state(true)
  let isSending = $state(false)
  let isUploading = $state(false)
  let isLoadingUploads = $state(false)
  let isIngesting = $state(false)
  let isClearing = $state(false)

  /*
    User-facing status/error strings.

    Keeping these in state means the template can render them immediately when
    a network request succeeds or fails.
  */
  let chatError = $state('')
  let databaseError = $state('')
  let databaseStatus = $state('No database action has run yet.')

  /*
    onMount runs once after this component has been inserted into the browser's
    DOM. That makes it a good place for initial browser-only work, such as
    fetching the available model list.
  */
  onMount(() => {
    void loadModels()
    void loadUploadedDocuments()
  })

  async function loadModels() {
    isLoadingModels = true
    chatError = ''

    try {
      /*
        This route already exists in the Rust server.

        Because the frontend and backend will be served from the same origin in
        production, a relative URL like "/v1/models" is enough.
      */
      const response = await checkedFetch('/v1/models')
      const body = (await response.json()) as ModelListResponse

      models = body.data?.map((model) => model.id) ?? []
      selectedModel = models[0] ?? ''
    } catch (error) {
      chatError = `Could not load models: ${messageFromError(error)}`
    } finally {
      isLoadingModels = false
    }
  }

  async function loadUploadedDocuments() {
    /*
      The upload picker chooses a file from the user's computer.

      This function asks the backend which files have already been uploaded to
      the server. Those uploaded records are what the ingest endpoint can work
      with.
    */
    isLoadingUploads = true
    databaseError = ''

    try {
      const response = await checkedFetch('/upload')
      const body = (await response.json()) as UploadListResponse

      uploadedDocuments = body.documents ?? []

      /*
        Keep any current selections that still exist on the server.

        We do not auto-select old uploads here. For ingestion, an explicit
        multi-selection is easier to reason about than a hidden default.
      */
      const uploadedIds = new Set(uploadedDocuments.map((document) => document.id))
      selectedDocumentIds = selectedDocumentIds.filter((id) => uploadedIds.has(id))
    } catch (error) {
      databaseError = `Could not load uploaded documents: ${messageFromError(error)}`
    } finally {
      isLoadingUploads = false
    }
  }

  async function sendMessage(event: SubmitEvent) {
    /*
      HTML forms normally submit by navigating the browser to a new page.

      preventDefault keeps this as an in-page app interaction so we can call the
      API with fetch() instead.
    */
    event.preventDefault()

    const content = prompt.trim()

    /*
      Guard clauses keep invalid actions from reaching the backend.

      The button is disabled in these cases too, but the function still checks
      because frontend event handlers can be called in other ways.
    */
    if (!content || !selectedModel || isSending) return

    const userMessage: ChatMessage = { role: 'user', content }
    const assistantMessage: ChatMessage = { role: 'assistant', content: '' }
    const nextMessages = [...messages, userMessage]

    /*
      Optimistic UI update:

      We immediately show the user's message and an empty assistant message.
      Then, as streaming chunks arrive, we append text into that assistant
      message so the user can watch the answer appear.
    */
    messages = [...nextMessages, assistantMessage]
    prompt = ''
    isSending = true
    chatError = ''

    try {
      /*
        This route already exists in the Rust server.

        The backend currently extracts the latest user message as the query.
        Sending the whole message list keeps this frontend close to the common
        OpenAI-style chat shape and leaves room for richer history later.
      */
      const response = await checkedFetch('/v1/chat/completions', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          model: selectedModel,
          messages: nextMessages,
          stream: true,
        }),
      })

      await readChatStream(response)
    } catch (error) {
      const message = messageFromError(error)

      chatError = message
      replaceLastAssistantMessage(`Sorry, the request failed: ${message}`)
    } finally {
      isSending = false
    }
  }

  async function readChatStream(response: Response) {
    /*
      The backend returns Server-Sent Events (SSE) for streaming chat.

      Each event looks roughly like:

      data: {"choices":[{"delta":{"content":"hello"}}]}

      and the stream ends with:

      data: [DONE]
    */
    const reader = response.body?.getReader()

    if (!reader) {
      throw new Error('The browser did not provide a readable response stream.')
    }

    const decoder = new TextDecoder()
    let buffer = ''

    while (true) {
      const { value, done } = await reader.read()

      /*
        TextDecoder turns Uint8Array network bytes into JavaScript strings.

        stream: true tells it that a multi-byte character may continue in the
        next chunk, which avoids corrupting Unicode text.
      */
      buffer += decoder.decode(value, { stream: !done })
      buffer = buffer.replace(/\r\n/g, '\n')

      let eventBoundary = buffer.indexOf('\n\n')
      while (eventBoundary !== -1) {
        const eventText = buffer.slice(0, eventBoundary)
        buffer = buffer.slice(eventBoundary + 2)

        if (handleChatStreamEvent(eventText)) {
          return
        }

        eventBoundary = buffer.indexOf('\n\n')
      }

      if (done) break
    }

    /*
      Most SSE streams end exactly on an event boundary. This handles a final
      leftover event defensively in case the server closes without a blank line.
    */
    if (buffer.trim() && handleChatStreamEvent(buffer)) {
      return
    }
  }

  function handleChatStreamEvent(eventText: string) {
    /*
      SSE events can contain fields other than "data:".

      This backend only sends data fields, so we collect those lines and ignore
      everything else.
    */
    const data = eventText
      .split('\n')
      .filter((line) => line.startsWith('data:'))
      .map((line) => line.slice('data:'.length).trimStart())
      .join('\n')
      .trim()

    if (!data) return false
    if (data === '[DONE]') return true

    const payload = JSON.parse(data) as ChatStreamChunk & ErrorResponse

    if (payload.error?.message) {
      throw new Error(payload.error.message)
    }

    const choice = payload.choices?.at(0)
    const content = choice?.delta?.content

    if (content) {
      appendToLastAssistantMessage(content)
    }

    return choice?.finish_reason !== undefined && choice.finish_reason !== null
  }

  function appendToLastAssistantMessage(content: string) {
    /*
      Svelte notices assignments to state variables.

      Instead of mutating messages[messages.length - 1].content directly, we
      create a new array with a replaced last message. That keeps reactivity
      obvious and predictable.
    */
    const lastMessage = messages.at(-1)

    if (lastMessage?.role !== 'assistant') return

    messages = [
      ...messages.slice(0, -1),
      {
        ...lastMessage,
        content: lastMessage.content + content,
      },
    ]
  }

  function replaceLastAssistantMessage(content: string) {
    /*
      Used when a streaming request fails. If we already created an assistant
      placeholder, replace its content with an error message.
    */
    const lastMessage = messages.at(-1)

    if (lastMessage?.role !== 'assistant') {
      messages = [...messages, { role: 'assistant', content }]
      return
    }

    messages = [
      ...messages.slice(0, -1),
      {
        ...lastMessage,
        content,
      },
    ]
  }

  function isLatestAssistantMessage(message: ChatMessage) {
    /*
      This helper marks the assistant message currently being streamed.

      It is used only for presentation: a temporary "Thinking..." placeholder
      before the first chunk, plus a small streaming cursor in CSS.
    */
    return isSending && message.role === 'assistant' && messages.at(-1) === message
  }

  function chooseFiles(event: Event) {
    /*
      The browser gives file inputs a FileList, which is array-like but not a
      true JavaScript array.

      Because this UI can upload multiple documents at once, Array.from(...)
      gives us a normal array that Svelte can render with {#each}.
    */
    const input = event.currentTarget as HTMLInputElement

    selectedFiles = Array.from(input.files ?? [])
    databaseError = ''
    databaseStatus =
      selectedFiles.length === 0
        ? 'No documents selected.'
        : `${formatCount(selectedFiles.length, 'document')} selected.`
  }

  async function uploadDocuments() {
    if (selectedFiles.length === 0 || isUploading) return

    /*
      FormData is the browser-native way to submit files with fetch().

      Do not manually set a Content-Type header for FormData. The browser adds
      the correct multipart boundary for us.
    */
    const form = new FormData()
    const files = selectedFiles

    for (const file of files) {
      form.append('documents', file)
    }

    isUploading = true
    databaseError = ''
    databaseStatus = `Uploading ${formatCount(files.length, 'document')}...`

    try {
      /*
        POST /upload accepts PDF/Markdown uploads and stores them in a staging
        area for ingestion. Each uploaded file is sent under the multipart form
        field named "documents".
      */
      const response = await checkedFetch('/upload', {
        method: 'POST',
        body: form,
      })
      const body = (await response.json()) as UploadResponse
      const uploaded = body.documents ?? []

      rememberUploadedDocuments(uploaded)
      selectedDocumentIds = uploaded.map((document) => document.id)
      databaseStatus = `${formatCount(uploaded.length, 'document')} uploaded and selected for ingestion.`
    } catch (error) {
      databaseError = messageFromError(error)
      databaseStatus = 'Upload failed.'
    } finally {
      isUploading = false
    }
  }

  async function ingestDocuments() {
    if (selectedDocumentIds.length === 0 || isIngesting) return

    const selectedDocuments = uploadedDocuments.filter((document) =>
      selectedDocumentIds.includes(document.id),
    )

    isIngesting = true
    databaseError = ''
    databaseStatus = `Ingesting ${formatCount(selectedDocumentIds.length, 'document')}...`

    try {
      /*
        PUT /ingest replaces the current RAG database with chunks from the
        selected uploaded documents.
      */
      const response = await checkedFetch('/ingest', {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ document_ids: selectedDocumentIds }),
      })
      const body = (await response.json()) as IngestResponse
      const ingestedCount = body.documents?.length ?? selectedDocuments.length

      databaseStatus = `${formatCount(ingestedCount, 'document')} ingested${
        body.chunk_count === undefined ? '.' : ` into ${body.chunk_count} chunks.`
      }`
    } catch (error) {
      databaseError = messageFromError(error)
      databaseStatus = 'Ingestion failed.'
    } finally {
      isIngesting = false
    }
  }

  function rememberUploadedDocuments(uploaded: UploadedDocument[]) {
    /*
      Add uploaded documents to the local list, replacing existing items with
      the same id if any are already present.

      The spread syntax creates a new array. That is helpful in reactive UI code
      because the assignment makes the state change obvious to Svelte.
    */
    const uploadedIds = new Set(uploaded.map((document) => document.id))
    uploadedDocuments = [
      ...uploaded,
      ...uploadedDocuments.filter((document) => !uploadedIds.has(document.id)),
    ]
  }

  async function clearDatabase() {
    if (isClearing) return

    /*
      confirm(...) is a built-in browser dialog.

      It is plain and not especially pretty, but it is useful while sketching
      dangerous actions because it prevents accidental clicks.
    */
    const confirmed = confirm('Clear the RAG database?')
    if (!confirmed) return

    isClearing = true
    databaseError = ''
    databaseStatus = 'Clearing database...'

    try {
      /*
        TODO: Add this route to the Rust server.

        Suggested behavior:
        DELETE /ingest removes the current vector/RAG database contents.
      */
      await checkedFetch('/ingest', { method: 'DELETE' })

      databaseStatus = 'Database cleared.'
    } catch (error) {
      databaseError = messageFromError(error)
      databaseStatus = 'Clear failed.'
    } finally {
      isClearing = false
    }
  }

  async function checkedFetch(input: RequestInfo | URL, init?: RequestInit) {
    /*
      fetch() only rejects for network-level failures.

      A 404 or 500 still counts as a successful HTTP response, so this helper
      converts non-2xx responses into thrown errors.
    */
    const response = await fetch(input, init)

    if (!response.ok) {
      throw new Error(await errorMessageFromResponse(response))
    }

    return response
  }

  async function errorMessageFromResponse(response: Response) {
    /*
      Many APIs return useful error text or JSON in the response body.

      For now we read it as text and fall back to the HTTP status if the body is
      empty. This keeps failures visible while the backend API is still forming.
    */
    const body = await response.text()
    return body.trim() || `${response.status} ${response.statusText}`
  }

  function renderMarkdown(markdown: string): string {
    const html = marked.parse(markdown) as string
    return DOMPurify.sanitize(html)
  }

  function messageFromError(error: unknown) {
    /*
      JavaScript lets anything be thrown, not just Error objects.

      This helper turns unknown thrown values into a string that is safe to show
      in the UI.
    */
    return error instanceof Error ? error.message : String(error)
  }

  function formatCount(count: number, noun: string) {
    /*
      Tiny display helper so status text reads naturally.
    */
    return `${count} ${noun}${count === 1 ? '' : 's'}`
  }

  function formatBytes(bytes: number) {
    /*
      File.size is measured in bytes.

      This makes file sizes easier to scan in the document list.
    */
    if (bytes < 1024) return `${bytes} B`
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
    return `${(bytes / 1024 / 1024).toFixed(1)} MB`
  }
</script>

<!--
  The whole app is one two-column layout:

  - The left/main section is the chat workflow.
  - The right/aside section is the RAG database workflow.
-->
<main class="app-shell">
  <!-- Chat region. The aria-labelledby link gives screen readers a useful name. -->
  <section class="chat-panel" aria-label="Document chat">
    <!-- {#if ...} conditionally renders markup only when the expression is truthy. -->
    {#if chatError}
      <p class="error">{chatError}</p>
    {/if}

    <!-- aria-live asks assistive tech to announce newly added messages. -->
    <div class="messages" aria-live="polite">
      {#if messages.length === 0}
        <p class="empty-state">Ask your first question to start a conversation.</p>
      {/if}

      <!-- {#each ...} repeats this block once for each chat message. -->
      {#each messages as message}
        <article
          class="message"
          class:from-user={message.role === 'user'}
          class:from-assistant={message.role === 'assistant'}
          class:is-streaming={isLatestAssistantMessage(message)}
        >
          <strong>{message.role === 'user' ? 'You' : 'Assistant'}</strong>

          {#if message.role === 'assistant'}
            {#if message.content}
              <div class="markdown">
                {@html renderMarkdown(message.content)}
              </div>
            {:else if isLatestAssistantMessage(message)}
              <p class="typing">Thinking...</p>
            {/if}
          {:else}
            <p>{message.content}</p>
          {/if}
        </article>
      {/each}
    </div>

    <!-- Submitting with Enter/Cmd+Enter behavior can be added later. -->
    <form class="composer" onsubmit={sendMessage}>
      <label for="prompt">Prompt</label>
      <textarea
        id="prompt"
        rows="4"
        bind:value={prompt}
        placeholder="Ask about your documents"
      ></textarea>

      <div class="composer-actions">
        <!-- bind:value keeps selectedModel and the <select> value synchronized. -->
        <label class="model-picker">
          <span>Model</span>
          <select bind:value={selectedModel} disabled={isLoadingModels || models.length === 0}>
            {#if isLoadingModels}
              <option value="">Loading models...</option>
            {:else if models.length === 0}
              <option value="">No models found</option>
            {:else}
              {#each models as model}
                <option value={model}>{model}</option>
              {/each}
            {/if}
          </select>
        </label>

        <button type="submit" disabled={isSending || !prompt.trim() || !selectedModel}>
          {isSending ? 'Sending...' : 'Send'}
        </button>
      </div>
    </form>
  </section>

  <!-- Database management region. Upload and ingest are wired; clear still needs backend logic. -->
  <aside class="database-panel" aria-labelledby="database-heading">
    <header>
      <h2 id="database-heading">RAG Database</h2>
      <p>Upload PDF or Markdown files, select which to ingest, or clear the database.</p>
    </header>

    <label class="file-picker">
      <span>Documents</span>
      <input
        type="file"
        multiple
        accept=".pdf,.md,.markdown,application/pdf,text/markdown"
        onchange={chooseFiles}
      />
    </label>

    {#if selectedFiles.length > 0}
      <ul class="selected-files" aria-label="Selected documents">
        {#each selectedFiles as file}
          <li>
            <span>{file.name}</span>
            <small>{formatBytes(file.size)}</small>
          </li>
        {/each}
      </ul>
    {:else}
      <p class="empty-state">No documents selected.</p>
    {/if}

    <label class="document-picker">
      <span>Uploaded Documents to Ingest</span>
      <select
        multiple
        size={Math.min(Math.max(uploadedDocuments.length, 3), 8)}
        bind:value={selectedDocumentIds}
        disabled={isLoadingUploads || uploadedDocuments.length === 0}
      >
        {#if isLoadingUploads}
          <option value="">Loading uploads...</option>
        {:else if uploadedDocuments.length === 0}
          <option value="">No uploaded documents</option>
        {:else}
          {#each uploadedDocuments as document}
            <option value={document.id}>{document.filename}</option>
          {/each}
        {/if}
      </select>
    </label>

    <div class="database-actions">
      <button type="button" onclick={uploadDocuments} disabled={selectedFiles.length === 0 || isUploading}>
        {isUploading ? 'Uploading...' : 'Upload'}
      </button>

      <button type="button" onclick={ingestDocuments} disabled={selectedDocumentIds.length === 0 || isIngesting}>
        {isIngesting ? 'Ingesting...' : 'Ingest'}
      </button>

      <button type="button" class="danger" onclick={clearDatabase} disabled={isClearing}>
        {isClearing ? 'Clearing...' : 'Clear Database'}
      </button>
    </div>

    {#if databaseError}
      <p class="error">{databaseError}</p>
    {/if}

    <p class="status">{databaseStatus}</p>
  </aside>
</main>
