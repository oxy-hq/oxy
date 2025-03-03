import type { Meta, StoryObj } from "@storybook/react";

import { TextFieldInput, TextFieldRoot, TextFieldSlot } from ".";
import Text from "@/components/ui/Typography/Text";
import { ROOT_DOMAIN } from "@/actions/onboarding/constants";

const meta: Meta<typeof TextFieldInput> = {
  component: TextFieldInput,
  argTypes: {
    disabled: {
      control: {
        type: "boolean"
      }
    },
    state: {
      control: {
        type: "select",
        options: ["default", "error"]
      }
    },
    placeholder: {
      control: {
        type: "text"
      }
    }
  },
  args: {
    disabled: false,
    placeholder: "Write here..."
  }
};

export default meta;
type Story = StoryObj<typeof TextFieldInput>;

export const DefaultWithValue: Story = {
  render: args => <TextFieldInput {...args} defaultValue="Hello world" />
};

export const DefaultNoValue: Story = {
  render: args => <TextFieldInput {...args} />
};

export const WithError: Story = {
  args: {
    state: "error"
  },
  render: args => <TextFieldInput {...args} defaultValue="Error here" />
};

export const Link: Story = {
  args: {
    state: "default"
  },
  render: args => (
    <TextFieldRoot slotVariant="link">
      <TextFieldSlot>
        <Text as="p" variant="label14Regular">
          {ROOT_DOMAIN}/
        </Text>
      </TextFieldSlot>
      <TextFieldInput
        {...args}
        placeholder="Company workspace"
        defaultValue="Hello world"
      />
    </TextFieldRoot>
  )
};
