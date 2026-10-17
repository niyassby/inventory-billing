import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./index.css"
import { TooltipProvider } from "@/components/ui/tooltip";
import { BrowserRouter } from "react-router-dom";
import { Toaster } from "./components/ui/sonner";
import QueryProvider from "./lib/query-client";

ReactDOM.createRoot(document.getElementById("root")).render(
  <React.StrictMode>
    <QueryProvider>
      <BrowserRouter>
        <TooltipProvider>
          <Toaster />
          <App />
        </TooltipProvider>
      </BrowserRouter>
    </QueryProvider>
  </React.StrictMode>,
);
