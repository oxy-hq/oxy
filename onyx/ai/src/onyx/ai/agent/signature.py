from dspy import InputField, OutputField, Signature


class AgentSignature(Signature):
    """Continue the conversation using the provided agent,
    carefully incorporating relevant information from the documents.
    Reply in markdown format.
    Cite sources inline when supporting your conclusions,
    using the `:s[<source_number>]` format for source numbers.
    If the conclusion is from multiple sources, use the format `:s[<source_number_1>]:s[<source_number_2>]...`.
    Example: :s[1]:s[3]:s[7]."""

    agent = InputField(format=lambda x: x)
    relevant_information = InputField(format=lambda x: x)
    chat_summary = InputField(format=lambda x: x)
    response = OutputField()


class AgentNoCitationSignature(Signature):
    """Continue the conversation using the provided agent,
    carefully incorporating relevant information from the documents.
    Reply in markdown format.
    Do not cite sources that support your conclusions."""

    agent = InputField(format=lambda x: x)
    relevant_information = InputField(format=lambda x: x)
    chat_summary = InputField(format=lambda x: x)
    response = OutputField()
