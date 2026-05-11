import { NexusLogoMini } from "@/components/brand/nexus-logo";
import { Loader2 } from "lucide-react";

export default function RootLoading() {
  return (
    <div className="flex items-center justify-center h-screen bg-nexus-deep">
      <div className="flex flex-col items-center gap-4">
        <NexusLogoMini size={48} />
        <Loader2 className="w-5 h-5 text-glow-cyan animate-spin" />
      </div>
    </div>
  );
}
