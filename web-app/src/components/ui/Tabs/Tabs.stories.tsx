import type { Meta, StoryObj } from "@storybook/react";
import { TabsRoot, TabsList, TabsTrigger, TabsContent } from ".";

const meta: Meta<typeof TabsRoot> = {
  component: TabsRoot
};

export default meta;

type Story = StoryObj<typeof TabsRoot>;

export const Default: Story = {
  render: () => (
    <TabsRoot defaultValue="tab1">
      <TabsList>
        <TabsTrigger iconAsset="search" value="tab1">
          Features
        </TabsTrigger>
        <TabsTrigger iconAsset="doc" value="tab2">
          Career coaching
        </TabsTrigger>
        <TabsTrigger iconAsset="user" value="tab3">
          Tech Thought Leaders
        </TabsTrigger>
        <TabsTrigger iconAsset="heart" value="tab4">
          Mindfulness
        </TabsTrigger>
      </TabsList>
      <TabsContent value="tab1">Tab 1 content</TabsContent>
      <TabsContent value="tab2">Tab 2 content</TabsContent>
      <TabsContent value="tab3">Tab 3 content</TabsContent>
      <TabsContent value="tab4">Tab 3 content</TabsContent>
    </TabsRoot>
  )
};
