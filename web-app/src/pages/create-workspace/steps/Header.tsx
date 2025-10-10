interface Props {
  title: string;
  description: string;
}

export default function Header({ title, description }: Props) {
  return (
    <div>
      <h2 className="text-xl font-semibold">{title}</h2>
      <p className="text-sm text-muted-foreground mt-1">{description}</p>
    </div>
  );
}
