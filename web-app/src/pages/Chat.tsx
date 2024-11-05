import React, { useEffect, useRef, useState } from "react";

import { useMutation } from "@tanstack/react-query";
import { ChatCompletionMessageParam } from "openai/resources/chat/completions";
import { css } from "styled-system/css";

const ChatApp = () => {
  const [messages, setMessages] = useState<ChatCompletionMessageParam[]>([]);
  const [input, setInput] = useState("");
  const [isLoading, setIsLoading] = useState(false);
  const messagesEndRef = useRef<HTMLDivElement>(null);

  const containerClass = css({ maxW: "4xl", mx: "auto", zIndex: "2" });
  const chatBoxClass = css({
    mb: "4",
    p: "6",
    border: "2px solid token(colors.primary)",
    borderRadius: "md",
    bg: "token(colors.background)"
  });
  const headerClass = css({
    display: "flex",
    justifyContent: "space-between",
    alignItems: "center",
    pb: "2",
    mb: "2",
    borderBottom: "1px solid token(colors.primary)",
    fontSize: "sm"
  });
  const messagesContainerClass = css({ h: "60vh", overflowY: "auto", mb: "4" });
  const messageClass = css({ mb: "4", _last: { mb: 0 } });
  const messageContentClass = css({ display: "flex", gap: "2" });
  const userRoleClass = css({ fontWeight: "bold", color: "token(colors.lightGray)" });
  const assistantRoleClass = css({ fontWeight: "bold", color: "token(colors.primary)" });
  const inputClass = css({
    w: "full",
    p: "2",
    bg: "transparent",
    color: "token(colors.primary)",
    outline: "none",
    caretColor: "token(colors.primary)",
    textTransform: "uppercase",
    "&::placeholder": { color: "rgba(0, 255, 0, 0.5)" }
  });
  const buttonClass = css({
    minW: "80px",
    borderRadius: "0px",
    px: "4",
    py: "2",
    backgroundColor: "token(colors.primary) !important",
    color: "token(colors.background) !important"
  });
  const footerLineClass = css({
    position: "absolute",
    bottom: "0",
    left: "0",
    right: "0",
    h: "1px",
    bg: "token(colors.primary)"
  });
  const propertyClass = css({ textAlign: "center", fontSize: "xs", color: "rgba(0, 255, 0, 0.7)" });

  const mutation = useMutation({
    mutationFn: async (messages: ChatCompletionMessageParam[]) => {
      const response = await fetch("https://api.openai.com/v1/chat/completions", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          Authorization: `Bearer ${import.meta.env.VITE_OPENAI_API_KEY}`
        },
        body: JSON.stringify({
          model: "gpt-3.5-turbo",
          messages,
          stream: true
        })
      });
      return response;
    }
  });

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages]);

  const processStreamChunk = async (
    chunk: string,
    accumulatedContent: string,
    setMessages: React.Dispatch<React.SetStateAction<ChatCompletionMessageParam[]>>
  ) => {
    const lines = chunk.split("\n");
    let newContent = accumulatedContent;

    for (const line of lines) {
      if (!line.startsWith("data: ")) continue;

      const jsonData = line.slice(6);
      if (jsonData === "[DONE]") break;

      try {
        const data = JSON.parse(jsonData);
        const content = data.choices[0]?.delta?.content || "";
        if (content) {
          newContent += content;
          setMessages((prev) => {
            const newMessages = [...prev];
            const lastMessage = newMessages[newMessages.length - 1];
            if (lastMessage.role === "assistant") {
              lastMessage.content = newContent;
            }
            return newMessages;
          });
        }
      } catch (error) {
        console.error("Error parsing JSON:", error);
      }
    }

    return newContent;
  };

  const handleStreamResponse = async (response: Response) => {
    const reader = response.body?.getReader();
    const decoder = new TextDecoder();

    setMessages((prev) => [...prev, { role: "assistant", content: "" }]);

    let accumulatedContent = "";
    while (true) {
      const { done, value } = (await reader?.read()) ?? { done: true, value: undefined };
      if (done) break;

      const chunk = decoder.decode(value, { stream: true });
      accumulatedContent = await processStreamChunk(chunk, accumulatedContent, setMessages);
    }
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!input.trim()) return;

    const userMessage: ChatCompletionMessageParam = { role: "user", content: input };
    setMessages((prev) => [...prev, userMessage]);
    setInput("");
    setIsLoading(true);

    try {
      const response = await mutation.mutateAsync([...messages, userMessage]);
      await handleStreamResponse(response);
    } catch (error) {
      console.error("Error:", error);
    } finally {
      setIsLoading(false);
    }
  };

  return (
    <div>
      <div className={containerClass}>
        <div className={chatBoxClass}>
          <div className={headerClass}>
            <div>ONYX PLC 2.4.00</div>
            <div>TERMINAL INFO: A-34-0.1</div>
          </div>

          <div className={messagesContainerClass}>
            {messages.map((message, index) => (
              <div key={index + message.role} className={messageClass}>
                <div className={messageContentClass}>
                  <span className={message.role === "user" ? userRoleClass : assistantRoleClass}>
                    {message.role === "user" ? "YOU:" : "SYSTEM:"}
                  </span>
                  <span
                    className={css({
                      flex: "1",
                      color:
                        message.role === "user"
                          ? "token(colors.lightGray)"
                          : "token(colors.primary)"
                    })}
                  >
                    {typeof message.content === "string"
                      ? message.content.toUpperCase()
                      : JSON.stringify(message.content).toUpperCase()}
                  </span>
                </div>
              </div>
            ))}
            <div ref={messagesEndRef} />
          </div>

          <form onSubmit={handleSubmit} className={css({ position: "relative" })}>
            <div className={css({ display: "flex", gap: "2" })}>
              <input
                type='text'
                value={input}
                onChange={(e) => setInput(e.target.value)}
                disabled={isLoading}
                className={inputClass}
                placeholder='ENTER MESSAGE...'
              />
              <button type='submit' disabled={isLoading} className={buttonClass}>
                SEND
              </button>
            </div>
            <div className={footerLineClass} />
          </form>
        </div>

        <div className={propertyClass}>PROPERTY OF ONYX SYSTEMS INC.</div>
      </div>
    </div>
  );
};

export default ChatApp;

