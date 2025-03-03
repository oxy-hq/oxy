import type { Meta, StoryObj } from "@storybook/react";

import {
  SelectRoot as Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from ".";

const meta: Meta<typeof Select> = {
  component: Select
};

export default meta;
type Story = StoryObj<typeof Select>;

export const DefaultWithNoValue: Story = {
  argTypes: {
    disabled: { control: "boolean", defaultValue: false }
  },
  render: args => (
    <Select {...args}>
      <SelectTrigger>
        <SelectValue placeholder="Select a fruit" />
      </SelectTrigger>
      <SelectContent>
        <SelectItem value="apple">Apple</SelectItem>
        <SelectItem value="banana">Banana</SelectItem>
        <SelectItem value="blueberry">Blueberry</SelectItem>
        <SelectItem value="grapes">Grapes</SelectItem>
        <SelectItem value="pineapple">Pineapple</SelectItem>
        <SelectItem value="strawberry">Strawberry</SelectItem>
        <SelectItem value="watermelon">Watermelon</SelectItem>
        <SelectItem value="pear">Pear</SelectItem>
        <SelectItem value="orange">Orange</SelectItem>
        <SelectItem value="mandarin">Mandarin</SelectItem>
        <SelectItem value="mango">Mango</SelectItem>
        <SelectItem value="lemon">Lemon</SelectItem>
        <SelectItem value="kiwi">Kiwi</SelectItem>
        <SelectItem value="grapefruit">Grapefruit</SelectItem>
        <SelectItem value="fig">Fig</SelectItem>
        <SelectItem value="elderberry">Elderberry</SelectItem>
        <SelectItem value="date">Date</SelectItem>
        <SelectItem value="cherry">Cherry</SelectItem>
        <SelectItem value="cantaloupe">Cantaloupe</SelectItem>
        <SelectItem value="blackberry">Blackberry</SelectItem>
        <SelectItem value="blackcurrant">Blackcurrant</SelectItem>
      </SelectContent>
    </Select>
  )
};

export const DefaultWithValue: Story = {
  args: {
    value: "banana"
  },
  render: args => (
    <Select {...args}>
      <SelectTrigger>
        <SelectValue placeholder="Select a fruit" />
      </SelectTrigger>
      <SelectContent>
        <SelectItem value="apple">Apple</SelectItem>
        <SelectItem value="banana">Banana</SelectItem>
        <SelectItem value="blueberry">Blueberry</SelectItem>
        <SelectItem value="grapes">Grapes</SelectItem>
        <SelectItem value="pineapple">Pineapple</SelectItem>
      </SelectContent>
    </Select>
  )
};

export const WithError: Story = {
  argTypes: {
    disabled: { control: "boolean", defaultValue: false }
  },
  args: {
    state: "error",
    disabled: false
  },
  render: args => (
    <Select {...args}>
      <SelectTrigger>
        <SelectValue placeholder="Select a fruit" />
      </SelectTrigger>
      <SelectContent>
        <SelectItem value="apple">Apple</SelectItem>
        <SelectItem value="banana">Banana</SelectItem>
        <SelectItem value="blueberry">Blueberry</SelectItem>
        <SelectItem value="grapes">Grapes</SelectItem>
        <SelectItem value="pineapple">Pineapple</SelectItem>
      </SelectContent>
    </Select>
  )
};
