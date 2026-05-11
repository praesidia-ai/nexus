import { FileQuestion, Home } from "lucide-react";
import Link from "next/link";

export default function NotFound() {
  return (
    <div className="flex items-center justify-center h-screen gradient-bg">
      <div className="max-w-md w-full mx-4">
        <div className="glass-card p-8 text-center">
          <div className="w-14 h-14 rounded-xl bg-glow-cyan/[0.08] border border-glow-cyan/20 flex items-center justify-center mx-auto mb-5">
            <FileQuestion className="w-7 h-7 text-glow-cyan" />
          </div>
          <h1 className="text-xl font-bold mb-2">Page not found</h1>
          <p className="text-sm text-slate-400 mb-6">
            The page you are looking for does not exist or has been moved.
          </p>
          <Link
            href="/"
            className="inline-flex items-center gap-2 px-4 py-2.5 rounded-lg bg-gradient-to-r from-glow-cyan to-glow-blue text-white text-sm font-medium hover:brightness-110 transition-colors"
          >
            <Home className="w-4 h-4" />
            Back to home
          </Link>
        </div>
      </div>
    </div>
  );
}
