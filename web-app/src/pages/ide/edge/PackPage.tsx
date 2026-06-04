import type React from "react";
import DomainPackCard from "./components/DomainPackCard";

const PackPage: React.FC = () => (
  <div className='flex h-full min-h-0 flex-col gap-3 p-4'>
    <DomainPackCard />
  </div>
);

export default PackPage;
