import { Bug, ExternalLink, Lightbulb, ListChecks, Mail } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";

/** Public repo that hosts issues + releases (source lives elsewhere). */
const REPO = "https://github.com/SmitSuthar8834/creatio-devhub";
const BUG_URL = `${REPO}/issues/new?labels=bug&template=bug_report.yml`;
const FEATURE_URL = `${REPO}/issues/new?labels=enhancement&template=feature_request.yml`;
const ISSUES_URL = `${REPO}/issues`;
const SUPPORT_EMAIL = "sutharsmit574@gmail.com";

type SupportCard = {
  icon: typeof Bug;
  title: string;
  description: string;
  action: string;
  onClick: () => void;
};

const CARDS: SupportCard[] = [
  {
    icon: Bug,
    title: "Report a bug",
    description: "Something not working right? Open a pre-filled bug report on GitHub.",
    action: "Report a bug",
    onClick: () => openUrl(BUG_URL),
  },
  {
    icon: Lightbulb,
    title: "Request a feature",
    description: "Have an idea that would make DevHub better? Suggest it on GitHub.",
    action: "Request a feature",
    onClick: () => openUrl(FEATURE_URL),
  },
  {
    icon: ListChecks,
    title: "Browse existing issues",
    description: "See what's already reported or planned before opening a new one.",
    action: "Browse issues",
    onClick: () => openUrl(ISSUES_URL),
  },
];

export default function SupportPage() {
  return (
    <div className="mx-auto w-full max-w-3xl space-y-6 p-6">
      <div>
        <h1 className="text-lg font-semibold">Support &amp; feedback</h1>
        <p className="mt-1 text-sm text-muted-foreground">
          Report a bug, request a feature, or reach out directly. Bug reports and feature requests
          open on GitHub in your browser.
        </p>
      </div>

      <div className="grid gap-4 sm:grid-cols-2">
        {CARDS.map((c) => (
          <Card key={c.title} className="flex flex-col">
            <CardHeader>
              <div className="flex size-9 items-center justify-center rounded-lg border bg-muted/50">
                <c.icon className="size-4.5 text-primary" aria-hidden="true" />
              </div>
              <CardTitle className="mt-3">{c.title}</CardTitle>
              <CardDescription>{c.description}</CardDescription>
            </CardHeader>
            <CardFooter className="mt-auto">
              <Button variant="outline" size="sm" onClick={c.onClick}>
                {c.action}
                <ExternalLink className="size-4" />
              </Button>
            </CardFooter>
          </Card>
        ))}

        <Card className="flex flex-col">
          <CardHeader>
            <div className="flex size-9 items-center justify-center rounded-lg border bg-muted/50">
              <Mail className="size-4.5 text-primary" aria-hidden="true" />
            </div>
            <CardTitle className="mt-3">Email the developer</CardTitle>
            <CardDescription>
              Prefer not to use GitHub? Send feedback or a question directly.
            </CardDescription>
          </CardHeader>
          <CardFooter className="mt-auto">
            <Button
              variant="outline"
              size="sm"
              onClick={() =>
                openUrl(
                  `mailto:${SUPPORT_EMAIL}?subject=${encodeURIComponent("Creatio DevHub — issue/feedback")}`,
                )
              }
            >
              {SUPPORT_EMAIL}
            </Button>
          </CardFooter>
        </Card>
      </div>

      <Card>
        <CardContent className="text-xs text-muted-foreground">
          Issues and releases are hosted on the public GitHub repository. You'll need a free GitHub
          account to post; browsing is open to everyone.
        </CardContent>
      </Card>
    </div>
  );
}
