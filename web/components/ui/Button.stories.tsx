import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { Button } from "./button";

const meta: Meta<typeof Button> = {
  title: "UI/Button",
  component: Button,
  parameters: {
    layout: "centered",
    docs: {
      description: {
        component:
          "JudicialPredict shadcn/ui Button — New York style, Slate base.",
      },
    },
  },
  tags: ["autodocs"],
  argTypes: {
    variant: {
      control: "select",
      options: [
        "default",
        "destructive",
        "outline",
        "secondary",
        "ghost",
        "link",
      ],
    },
    size: {
      control: "select",
      options: ["default", "sm", "lg", "icon"],
    },
    disabled: { control: "boolean" },
  },
};

export default meta;
type Story = StoryObj<typeof Button>;

export const Default: Story = {
  args: {
    children: "Evaluate Case",
    variant: "default",
    size: "default",
  },
};

export const Outline: Story = {
  args: {
    children: "Export Memo",
    variant: "outline",
  },
};

export const Destructive: Story = {
  args: {
    children: "Delete Case",
    variant: "destructive",
  },
};

export const Small: Story = {
  args: {
    children: "View",
    size: "sm",
  },
};

export const Disabled: Story = {
  args: {
    children: "Unavailable",
    disabled: true,
  },
};
