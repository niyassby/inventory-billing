import React from "react";
import { useNavigate } from "react-router-dom";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  Tag01Icon
} from "@hugeicons/core-free-icons";

export default function Returns() {
  const navigate = useNavigate();

  return (
    <div>

      <div className="space-y-4">
        <h3 className="text-lg font-semibold">Sales Returns & Credit Notes</h3>
        
        <Card className="border-border/40 bg-card/65 backdrop-blur-sm">
          <CardContent className="flex flex-col items-center justify-center p-12 text-center">
            <div className="h-12 w-12 rounded-full bg-primary/10 text-primary flex items-center justify-center mb-4">
              <HugeiconsIcon icon={Tag01Icon} size={24} />
            </div>
            <h4 className="text-md font-semibold text-foreground mb-1">No Returns Processed</h4>
            <p className="text-sm text-muted-foreground max-w-sm">
              All inventory sales are completed. When you process a customer return, credit notes and refunds will appear here.
            </p>
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
