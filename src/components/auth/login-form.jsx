import { cn } from "@/lib/utils"
import { Button } from "@/components/ui/button"
import {
  Field,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { useState } from "react";
import { useStore } from "@/hooks/use-store";
import { useNavigate } from "react-router-dom";

export function LoginForm({
  className,
  ...props
}) {
  const [formData, setFormData] = useState({
    email: "admin@example.com",
    password: "password123",
  })
  const [errorMsg, setErrorMsg] = useState("")
  const login = useStore((state) => state.login);
  const navigate = useNavigate();

  const onsubmit = async (e) => {
    e.preventDefault();
    setErrorMsg("");
    const res = await login(formData.email, formData.password);
    if (res.success) {
      navigate("/dashboard");
    } else {
      setErrorMsg(res.error || "Invalid credentials. Please try again.");
    }
  }

  return (
    <form onSubmit={onsubmit} className={cn("flex flex-col gap-6", className)} {...props}>
      <FieldGroup>
        <div className="flex flex-col items-center gap-1 text-center">
          <h1 className="text-2xl font-bold">Sign In</h1>
          <p className="text-sm text-balance text-muted-foreground">
            Enter your credentials to access your offline dashboard
          </p>
        </div>
        
        {errorMsg && (
          <div className="p-3 text-sm text-destructive bg-destructive/15 rounded-md border border-destructive/25 text-center">
            {errorMsg}
          </div>
        )}

        <Field>
          <FieldLabel htmlFor="email">Email Address</FieldLabel>
          <Input 
            value={formData.email}
            onChange={(e) => setFormData({ ...formData, email: e.target.value })} 
            id="email" 
            type="email" 
            placeholder="admin@example.com" 
            required 
          />
        </Field>
        
        <Field>
          <div className="flex items-center justify-between">
            <FieldLabel htmlFor="password">Password</FieldLabel>
          </div>
          <Input 
            value={formData.password}
            onChange={(e) => setFormData({ ...formData, password: e.target.value })} 
            id="password" 
            type="password" 
            placeholder="••••••••"
            required 
          />
        </Field>

        <Field>
          <Button type="submit" className="w-full">Sign In</Button>
        </Field>

        <div className="text-xs text-center text-muted-foreground mt-2 border-t pt-4 border-border/40">
          <p className="font-semibold text-foreground mb-1">Demo Credentials:</p>
          <p>Email: <span className="font-mono text-foreground">admin@example.com</span></p>
          <p>Password: <span className="font-mono text-foreground">password123</span></p>
        </div>
      </FieldGroup>
    </form>
  );
}
