interface Props {
  title: string;
  description: string;
}

export default function Header({ title, description }: Props) {
  return (
    <div>
      <h2 className='font-semibold text-xl'>{title}</h2>
      <p className='mt-1 text-muted-foreground text-sm'>{description}</p>
    </div>
  );
}
