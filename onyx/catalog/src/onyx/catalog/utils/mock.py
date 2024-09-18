import uuid

# only apply buffer to these organizations
# avoid user created agents from being affected
buffered_organization_ids = {
    "f68efd6b-909a-4859-a1ab-1565ae46cfdc",  # hello@hyperquery.ai on prod,
    "d5df269a-2a7c-42de-9245-1d9ebbe8ce33",  # hello@hyperquery.ai on dev,
    "adfd5112-1547-485a-8e36-c80b0fa4558b",  # hello@hyperquery.ai on demo,
}


# buffer to boost up fake numbers
# see ENG-1059
def get_buffer_numbers(agent_id, organization_id: str):
    if str(organization_id) not in buffered_organization_ids:
        return 0, 0
    if isinstance(agent_id, str):
        agent_id = uuid.UUID(agent_id)
    likes_buffer = agent_id.int % 1000
    messages_buffer = agent_id.int % 5000 + likes_buffer * 2
    return likes_buffer, messages_buffer
