from onyx.catalog.adapters.search import AbstractSearchClient


def test_is_search_agent_question(fake_search_client: AbstractSearchClient):
    assert fake_search_client.is_search_agent("who is the best persona") is False
    assert fake_search_client.is_search_agent("what is the best persona") is False
    assert fake_search_client.is_search_agent("where is the best persona") is False
    assert fake_search_client.is_search_agent("when is the best persona") is False
    assert fake_search_client.is_search_agent("why is the best persona") is False
    assert fake_search_client.is_search_agent("how is the best persona") is False
    assert fake_search_client.is_search_agent("which is the best persona") is False
    assert fake_search_client.is_search_agent("whom is the best persona") is False
    assert fake_search_client.is_search_agent("whose is the best persona") is False
    assert fake_search_client.is_search_agent("whether is the best persona") is False
    assert fake_search_client.is_search_agent("is the best persona?") is False
    assert fake_search_client.is_search_agent("the best persona?") is False
    assert fake_search_client.is_search_agent("best persona?") is False
    assert fake_search_client.is_search_agent("is the best persona") is True
    assert fake_search_client.is_search_agent("the best persona") is True
