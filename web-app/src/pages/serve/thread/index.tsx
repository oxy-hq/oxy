import { useParams } from "react-router-dom";

const Thread = () => {
  const { threadId } = useParams();
  return <div>Thread {threadId}</div>;
};

export default Thread;
