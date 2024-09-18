from hypothesis import strategies as st
from onyx.catalog.features.data_sources.commands import CreateIntegration
from onyx.catalog.models.commands import (
    IntegrationConfiguration,
)
from onyx.shared.models.constants import IntegrationSlugChoices

create_integration_command = st.builds(
    CreateIntegration,
    organization_id=st.uuids(version=4),
    name=st.text(),
    configuration=st.one_of(
        st.builds(
            IntegrationConfiguration,
            slug=st.just(IntegrationSlugChoices.file),
            path=st.text(),
        ),
        st.builds(
            IntegrationConfiguration,
            slug=st.just(IntegrationSlugChoices.gmail),
            refresh_token=st.text(),
            query=st.text(),
        ),
        st.builds(
            IntegrationConfiguration,
            slug=st.just(IntegrationSlugChoices.notion),
            token=st.text(),
        ),
        st.builds(
            IntegrationConfiguration,
            slug=st.just(IntegrationSlugChoices.slack),
            token=st.text(),
        ),
        st.builds(
            IntegrationConfiguration,
            slug=st.just(IntegrationSlugChoices.salesforce),
            refresh_token=st.text(),
        ),
    ),
)
