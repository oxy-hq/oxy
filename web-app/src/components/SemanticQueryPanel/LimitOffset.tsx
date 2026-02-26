interface LimitOffsetProps {
  limit?: number;
  offset?: number;
}

const LimitOffset = ({ limit, offset }: LimitOffsetProps) => {
  if (limit == null && offset == null) return null;

  return (
    <div className='flex items-center gap-3 border-border border-b px-3 py-2'>
      {limit != null && (
        <div className='flex items-center gap-1.5 text-xs'>
          <span className='text-muted-foreground'>Limit:</span>
          <span className='font-mono'>{limit}</span>
        </div>
      )}
      {offset != null && offset > 0 && (
        <div className='flex items-center gap-1.5 text-xs'>
          <span className='text-muted-foreground'>Offset:</span>
          <span className='font-mono'>{offset}</span>
        </div>
      )}
    </div>
  );
};

export default LimitOffset;
