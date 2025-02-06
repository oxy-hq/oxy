const getReaderFromResponse = async (response: Response) => {
  if (!response.ok) {
    throw new Error(
      (await response.text()) || "Failed to fetch the chat response.",
    );
  }

  if (!response.body) {
    throw new Error("The response body is empty.");
  }
  const reader = response.body.getReader();
  return reader;
};

export const readMessageFromStreamData = async <T>(
  response: Response,
  onReadStream: (message: T) => void,
) => {
  const reader = await getReaderFromResponse(response);
  for await (const line of makeTextLineIterator(reader)) {
    if (!line) {
      return;
    }
    const message = JSON.parse(line);
    if (message) {
      onReadStream(message);
    }
  }
};

// Reference:
// https://developer.mozilla.org/en-US/docs/Web/API/ReadableStreamDefaultReader/read#example_2_-_handling_text_line_by_line
async function* makeTextLineIterator(reader: ReadableStreamDefaultReader) {
  const utf8Decoder = new TextDecoder("utf-8");
  let { value: chunk, done: readerDone } = await reader.read();
  chunk = chunk ? utf8Decoder.decode(chunk, { stream: true }) : "";

  const re = /\r\n|\n|\r/gm;
  let startIndex = 0;

  for (;;) {
    const result = re.exec(chunk);
    if (!result) {
      if (readerDone) {
        break;
      }
      const remainder = chunk.substr(startIndex);
      ({ value: chunk, done: readerDone } = await reader.read());
      chunk =
        remainder + (chunk ? utf8Decoder.decode(chunk, { stream: true }) : "");
      startIndex = re.lastIndex = 0;
      continue;
    }
    yield chunk.substring(startIndex, result.index);
    startIndex = re.lastIndex;
  }
  if (startIndex < chunk.length) {
    // last line didn't end in a newline char
    yield chunk.substr(startIndex);
  }
}
